use crate::lang::has_language;
use crate::response::{Response, ResponseError, WetRecord};
use crate::{crawl_utils, CrawlEntry, lang, ScrapEntry};
use async_channel::Receiver as AsyncReceiver;
use futures::future::join_all;
use libflate::gzip::Encoder;
use reqwest::Error;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel as std_channel, Sender, TryRecvError, Receiver, RecvError, SendError};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tokio::sync::RwLock as AsyncRwLock;
use warc::WarcWriter;
use whatlang::{Detector, Lang};

type WetFile = Arc<WarcWriter<std::io::BufWriter<Encoder<std::fs::File>>>>;

pub fn start_crawl(seeds: Vec<CrawlEntry>, file: &str, crawler_count: usize) {
    let mut wet_file = WarcWriter::from_path_gzip(file).unwrap();
    let (tx_processor_writer, rx_bgwriter) = std_channel();
    let (tx_processor, rx_crawler) = async_channel::unbounded();
    let (tx_crawler, rx_processor) = std_channel::<ScrapEntry>();
    let visited_urls = Arc::new(AsyncRwLock::new(HashSet::new()));
    let visited_url_count = Arc::new(AtomicU32::new(0));
    let bad_url_count = Arc::new(AtomicU32::new(0));
    let model_langs = vec!["arabic", "english"];
    let lang_detector = lang::build_langdetector(model_langs);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // let tx_ld = tx_loader.clone();
    for x in seeds {
        tx_processor.send_blocking(x).unwrap();
    }

    let stall_flags = Arc::new(RwLock::new(vec![false; crawler_count + 1]));
    let mut crawlers = Vec::with_capacity(crawler_count);
    for idx in 0..crawler_count {
        crawlers.push(crawl_url(
            idx,
            rx_crawler.clone(),
            tx_crawler.clone(),
            visited_urls.clone(),
            visited_url_count.clone(),
            bad_url_count.clone(),
            Duration::from_millis(8000),
            stall_flags.clone(),
        ));
    }
    // let rx_processor = Arc::new(rx_processor);
    rt.spawn_blocking( move  || {
        println!("entered proc");
        let mut total_extra = 0;
        loop {
            match &rx_processor {
                Ok(crawled) => {
                    println!("PAGE!!!");
                    *stall_flags.write().unwrap().last_mut().unwrap() = false;
                    let out = process_crawled(&crawled,&lang_detector,Lang::Ara);
                    tx_processor_writer.send(out.0).unwrap();
                    if let Some(links) = out.1{
                        println!("got extra {} links",links.len());
                        total_extra += links.len();
                            for x in links {
                                tx_processor.send_blocking(x).unwrap()
                            }

                    }
                }
                Err(e) => {
                    println!("proc??");
                    match e {
                    TryRecvError::Empty => {
                        if stall_flags.read().unwrap().iter().all(|val| *val){
                            println!("empty proc q");
                            break
                        }
                        else {
                            println!("no");
                            *stall_flags.write().unwrap().last_mut().unwrap() = true;
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                    TryRecvError::Disconnected => break
                }},
            }
        }
        println!("exited processor");
        println!("total extra : {total_extra}");
    });
    rt.spawn_blocking(|| background_writer(rx_bgwriter,wet_file));
    let (ctr1,ctr2) = (visited_url_count.clone(),bad_url_count.clone());
    rt.spawn(async move {
        loop {
            println!("visited :{} \t bad:{}",ctr1.load(Ordering::Relaxed),ctr2.load(Ordering::Relaxed));
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
    });
    rt.block_on(join_all(crawlers));
    println!("visited :{} \t bad:{}",visited_url_count.load(Ordering::Relaxed),bad_url_count.load(Ordering::Relaxed));
}

async fn crawl_url(
    idx: usize,
    rx_url: AsyncReceiver<CrawlEntry>,
    tx_page: Sender<ScrapEntry>,
    visited_urls: Arc<AsyncRwLock<HashSet<String>>>,
    visited_count: Arc<AtomicU32>,
    bad_url_count: Arc<AtomicU32>,
    timeout: Duration,
    stall_flags: Arc<RwLock<Vec<bool>>>,
) {
    let client: reqwest::Client = reqwest::Client::new();
    loop {
        match rx_url.try_recv() {
            Ok(crawl_entry) => {
                // println!("got a link");
                stall_flags.write().unwrap()[idx] = false;
                if visited_urls.read().await.contains(&crawl_entry.url) {
                    continue;
                }
                let  resp = client.get(&crawl_entry.url).timeout(timeout).send().await;
                visited_urls.write().await.insert(crawl_entry.url);
                let response = match resp {
                    Ok(resp) => match Response::from_request(resp).await {
                        Ok(resp) => Some(resp),
                        Err(_) => None
                    }
                    Err(_) => None
                };

                if let Some(response) = response {
                    let scrap_entry = ScrapEntry {
                        response,
                        crawl_depth: crawl_entry.crawl_depth - 1,
                    };
                    if let Err(e) = tx_page.send(scrap_entry){
                        println!("{e:?}");
                    }
                    visited_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    bad_url_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => match e {
                async_channel::TryRecvError::Empty => {
                    // println!("NO MORE");
                    if stall_flags.read().unwrap().iter().all(|val| *val) {
                        println!("BYE boy");
                        return;
                    } else {
                        stall_flags.write().unwrap()[idx] = true;
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
                async_channel::TryRecvError::Closed => return,
            },
        }
    }
}

fn process_crawled(
    response: &ScrapEntry,
    lang_detector: &Detector,
    accept_langs: Lang,
) -> (WetRecord, Option<Vec<CrawlEntry>>) {
    let soup = response.response.to_soup();
    let wet_record = response.response.to_warcrecord(Some(&soup));
    let outlinks = if response.crawl_depth != 0
        && has_language(lang_detector, &wet_record.body, accept_langs, 4)
    {
        Some(
            crawl_utils::soup_links(&soup, &[])
                .into_iter()
                .map(|link| CrawlEntry {
                    url: link,
                    crawl_depth: response.crawl_depth,
                })
                .collect::<Vec<CrawlEntry>>(),
        )
    } else {
        None
    };
    (wet_record, outlinks)
}
fn background_writer(records: Receiver<WetRecord>, mut warc: WarcWriter<BufWriter<Encoder<File>>>){
    loop {
        match records.recv() {
            Ok(rec) =>warc.write_raw(rec.headers,&rec.body).unwrap(),
            Err(_) => break
        };
    }
    unsafe {
        let gzip_stream = warc.into_inner().unwrap_unchecked();
        gzip_stream.finish().unwrap();
    }
}
