use crate::lang::has_language;
use crate::response::{Response, ResponseError, WetRecord};
use crate::{crawl_utils, lang, CrawlEntry, ScrapEntry};
use async_channel::Receiver as AsyncReceiver;
use futures::future::join_all;
use libflate::gzip::Encoder;
use reqwest::Error;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{
    channel as std_channel, Receiver, RecvError, SendError, Sender, TryRecvError,
};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use config::Config;
use tokio::sync::RwLock as AsyncRwLock;
use warc::WarcWriter;
use whatlang::{Detector, Lang};

type WetFile = Arc<WarcWriter<std::io::BufWriter<Encoder<std::fs::File>>>>;

pub fn start_crawl(seeds: Vec<CrawlEntry>, job :&Config) {
    let (warc_dst,crawler_count,link_timeout,accept_langs ) =
        (job.get_string("destination_warc").unwrap(),
         job.get_int("crawl_tasks").unwrap() as usize,
         job.get_int("link_timeout").unwrap() as u64,
         job.get_array("accept_languages").unwrap().into_iter().map(|value| value.into_string().unwrap()).collect::<Vec<String>>()
        );
    let wet_file = WarcWriter::from_path_gzip(warc_dst).unwrap();
    let (tx_processor_writer, rx_bgwriter) = std_channel();
    let (tx_processor, rx_crawler) = async_channel::unbounded();
    let (tx_crawler, rx_processor) = std_channel::<ScrapEntry>();
    let visited_urls = Arc::new(AsyncRwLock::new(HashSet::new()));
    let visited_url_count = Arc::new(AtomicU32::new(0));
    let bad_url_count = Arc::new(AtomicU32::new(0));
    let model_langs = vec!["arabic", "english"];
    let accept_langs = lang::lang_builder(accept_langs.iter().map(|lang|lang.as_str()).collect());
    let lang_detector = lang::build_langdetector(model_langs);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // let tx_ld = tx_loader.clone();
    let initial = seeds.len();
    for x in seeds {
        tx_processor.send_blocking(x).unwrap();
    }

    let started_crawling = Arc::new(AtomicBool::new(false));

    let mut crawlers = Vec::with_capacity(crawler_count);
    for idx in 0..crawler_count {
        crawlers.push(crawl_url(
            idx,
            rx_crawler.clone(),
            tx_crawler.clone(),
            visited_urls.clone(),
            visited_url_count.clone(),
            bad_url_count.clone(),
            Duration::from_millis(link_timeout ),
            started_crawling.clone(),
        ));
    }
    let total_extra = Arc::new(AtomicUsize::default());
    let send = total_extra.clone();
    rt.spawn_blocking(move || {
        println!("entered proc");
        while !started_crawling.load(Ordering::Relaxed) {
            sleep(Duration::from_secs(5));
        }
        loop {
            match &rx_processor.recv_timeout(Duration::from_secs(60)) {
                Ok(crawled) => {
                    let out = process_crawled(crawled, &lang_detector, &accept_langs);
                    tx_processor_writer.send(out.0).unwrap();
                    if let Some(links) = out.1 {
                        send.fetch_add(links.len(),Ordering::Relaxed);
                        for x in links {
                            tx_processor.send_blocking(x).unwrap()
                        }
                    }
                }
                Err(e) => {if tx_processor.is_empty(){break}else { continue }},
            }
        }
        println!("exited processor");
        println!("total extra : {}",send.load(Ordering::Relaxed));
    });
    rt.spawn_blocking(|| background_writer(rx_bgwriter, wet_file));
    let (ctr1, ctr2) = (visited_url_count.clone(), bad_url_count.clone());
    let extra = total_extra.clone();
    rt.spawn(async move {
        loop {
            println!(
                "visited :{} \t bad:{}\t extra:{}",
                ctr1.load(Ordering::Relaxed),
                ctr2.load(Ordering::Relaxed),
                extra.load(Ordering::Relaxed)
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    rt.block_on(join_all(crawlers));

    println!(
        "visited :{} \t bad:{}\t\
        initial  :{} \t total_extra:{}\t\
        set:{}
        ",
        visited_url_count.load(Ordering::Relaxed),
        bad_url_count.load(Ordering::Relaxed),
        initial,total_extra.load(Ordering::Relaxed),
        visited_urls.blocking_read().len()
    );
}

async fn crawl_url(
    idx: usize,
    rx_url: AsyncReceiver<CrawlEntry>,
    tx_page: Sender<ScrapEntry>,
    visited_urls: Arc<AsyncRwLock<HashSet<String>>>,
    visited_count: Arc<AtomicU32>,
    bad_url_count: Arc<AtomicU32>,
    timeout: Duration,
    started_crawling: Arc<AtomicBool>,
) {
    let client: reqwest::Client = reqwest::Client::builder().timeout(timeout).build().unwrap();
    started_crawling.store(true, Ordering::Relaxed);
    loop {
        match rx_url.recv().await {
            Ok(crawl_entry) => {
                if visited_urls.read().await.contains(&crawl_entry.url) {
                    continue;
                }
                let resp = client.get(&crawl_entry.url).send().await;
                visited_urls.write().await.insert(crawl_entry.url.clone());
                let response = match resp {
                    Ok(resp) => match Response::from_request(resp).await {
                        Ok(resp) => Some(resp),
                        Err(_) => None,
                    },
                    Err(_) => None,
                };

                if let Some(response) = response {
                    let scrap_entry = ScrapEntry {
                        response,
                        crawl_depth: crawl_entry.crawl_depth - 1,
                    };
                    if let Err(e) = tx_page.send(scrap_entry) {
                        println!("{e:?}");
                    }
                    visited_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    bad_url_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(e) => {
                println!("crawl bye {idx}");
                break;
            }
        }
    }
}

fn process_crawled(
    response: &ScrapEntry,
    lang_detector: &Detector,
    accept_langs: &Vec<Lang>,
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
fn background_writer(records: Receiver<WetRecord>, mut warc: WarcWriter<BufWriter<Encoder<File>>>) {
    while let Ok(rec) = records.recv() {
        warc.write_raw(rec.headers, &rec.body).unwrap();
    }
    println!("finish");
    unsafe {
        let gzip_stream = warc.into_inner().unwrap_unchecked();
        gzip_stream.finish().unwrap();
    }
}
