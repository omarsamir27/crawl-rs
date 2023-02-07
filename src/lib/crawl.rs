use crate::lang::has_language;
use crate::response::{Response, ResponseError, WetRecord};
use crate::robots::{Robots, RobotsVerdict};
use crate::{crawl_utils, lang, CrawlEntry, ScrapEntry};
use async_channel::Receiver as AsyncReceiver;
use async_channel::Sender as AsyncSender;
use config::Config;
use futures::future::join_all;
use libflate::gzip::Encoder;
use reqwest::{Client, Error};
use std::collections::HashSet;
use std::fs::File;
use std::future::Future;
use std::io::BufWriter;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{
    channel as std_channel, Receiver, RecvError, SendError, Sender, TryRecvError,
};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::{mem, thread};
use std::thread::sleep;
use std::time::Duration;
use ahash::AHashSet;
use texting_robots::Robot;
use tokio::sync::RwLock as AsyncRwLock;
use url::Url;
use warc::WarcWriter;
use whatlang::{Detector, Lang};
use crate::crawl_utils::disperse_domains;

type WetFile = Arc<WarcWriter<std::io::BufWriter<Encoder<std::fs::File>>>>;

pub fn start_crawl(seeds: Vec<CrawlEntry>, job: &Config) {
    let (warc_dst, crawler_count, link_timeout, accept_langs) = (
        job.get_string("destination_warc").unwrap(),
        job.get_int("crawl_tasks").unwrap() as usize,
        job.get_int("link_timeout").unwrap() as u64,
        job.get_array("accept_languages")
            .unwrap()
            .into_iter()
            .map(|value| value.into_string().unwrap())
            .collect::<Vec<String>>(),
    );
    let wet_file = WarcWriter::from_path_gzip(warc_dst).unwrap();
    let (tx_processor_writer, rx_bgwriter) = std_channel();
    let (tx_processor, rx_crawler) = async_channel::unbounded();
    let (tx_crawler, rx_processor) = std_channel::<ScrapEntry>();
    let visited_urls = Arc::new(AsyncRwLock::new(HashSet::new()));
    let visited_url_count = Arc::new(AtomicU32::new(0));
    let bad_url_count = Arc::new(AtomicU32::new(0));
    let model_langs = vec!["arabic", "english"];
    let accept_langs = lang::lang_builder(accept_langs.iter().map(|lang| lang.as_str()).collect());
    let lang_detector = lang::build_langdetector(model_langs);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let initial = seeds.len();
    let mut known_urls: AHashSet<String> = seeds.iter().map(|entry| entry.url.clone()).collect();
    for x in seeds {
        tx_processor.send_blocking(x).unwrap();
    }

    let started_crawling = Arc::new(AtomicBool::new(false));

    let mut crawlers = Vec::with_capacity(crawler_count);
    let client: reqwest::Client = reqwest::Client::builder()
        .pool_max_idle_per_host(1)
        .pool_idle_timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_millis(link_timeout))
        .build()
        .unwrap();
    let robots = Arc::new(Robots::new());
    for idx in 0..crawler_count {
        crawlers.push(crawl_url(
            client.clone(),
            idx,
            rx_crawler.clone(),
            tx_crawler.clone(),
            visited_urls.clone(),
            visited_url_count.clone(),
            bad_url_count.clone(),
            started_crawling.clone(),
            robots.clone(),
            tx_processor.clone(),
        ));
    }
    let total_extra = Arc::new(AtomicUsize::default());
    let send = total_extra.clone();
    let accept_all = accept_langs.is_empty();
    rt.spawn_blocking(move || {
        println!("entered proc");
        let mut link_cache = Vec::new();
        while !started_crawling.load(Ordering::Relaxed) {
            sleep(Duration::from_secs(5));
        }
        loop {
            match &rx_processor.recv_timeout(Duration::from_secs(60)) {
                Ok(crawled) => {
                    let out = process_crawled(crawled, &lang_detector, &accept_langs, accept_all);
                    tx_processor_writer.send(out.0).unwrap();
                    if let Some(mut links) = out.1 {
                        links.retain(|i| known_urls.insert(i.url.clone()));
                        send.fetch_add(links.len(), Ordering::Relaxed);
                        if link_cache.len() >= 400{
                            let cached = mem::take(&mut link_cache);
                            let dispersed = disperse_domains(cached);
                            for x in dispersed {
                                tx_processor.send_blocking(x).unwrap()
                            }
                        }
                        else {
                            link_cache.append(&mut links)
                        }

                    }
                }
                Err(e) => {
                    if tx_processor.is_empty() {
                        break;
                    } else {
                        continue;
                    }
                }
            }
        }
        println!("exited processor");
        println!("total extra : {}", send.load(Ordering::Relaxed));
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
        initial,
        total_extra.load(Ordering::Relaxed),
        visited_urls.blocking_read().len()
    );
}

async fn crawl_url(
    client: Client,
    idx: usize,
    rx_url: AsyncReceiver<CrawlEntry>,
    tx_page: Sender<ScrapEntry>,
    visited_urls: Arc<AsyncRwLock<HashSet<String>>>,
    visited_count: Arc<AtomicU32>,
    bad_url_count: Arc<AtomicU32>,
    started_crawling: Arc<AtomicBool>,
    robots: Arc<Robots>,
    loopback: AsyncSender<CrawlEntry>,
) {
    started_crawling.store(true, Ordering::Relaxed);
    loop {
        match rx_url.recv().await {
            Ok(crawl_entry) => {
                /*
                ask if url is valid,
                if yes : ask if domain robots has been saved , if yes : ask if can visit again
                else : add domain rules , then ask if can visit
                 */
                let (verdict, domain,malformed_url) =
                    eval_robots(&client, &crawl_entry.url, &robots).await;
                match verdict {
                    None if malformed_url => continue,
                    None if !malformed_url => robots.update_domain(&domain.unwrap()).await,
                    None => continue,
                    Some(verdict) => match verdict {
                        RobotsVerdict::ForbiddenPath => continue,
                        RobotsVerdict::CrawlDelay => {
                            loopback.send(crawl_entry).await.unwrap();
                            continue;
                        }
                        RobotsVerdict::Proceed => robots.update_domain(&domain.unwrap()).await,
                    },
                }
                if visited_urls.read().await.contains(&crawl_entry.url) {
                    continue;
                }
                let resp = client.get(&crawl_entry.url).send().await;
                visited_urls.write().await.insert(crawl_entry.url.clone());
                let response = match resp {
                    Ok(resp) => match Response::from_request(resp).await {
                        Ok(resp) => Some(resp),
                        Err(e) => {
                            eprintln!("{e:?}");
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("{e:?}");
                        None
                    }
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

async fn eval_robots(
    client: &Client,
    url: &str,
    robots: &Arc<Robots>,
) -> (Option<RobotsVerdict>, Option<Url>,bool) {
    if let Ok(url) = Robots::valid_url(url) &&
        let Some(domain) = Robots::to_domain(&url){
             if robots.has_domain(&domain).await{
                 (Some(robots.can_visit_url(&url, &domain).await),Some(domain),false)
             }else {
                 let robots_url = Robots::robots_url(&domain);
                 let txt = url_to_text(client,robots_url.as_str()).await;
                 if txt.is_err(){return (None,None,false)}
                 let txt = txt.unwrap();
                 if robots.insert_entry(&domain,txt.as_str()).await {
                     (Some(robots.can_visit_url(&url,&domain).await),Some(domain),false)
                 }else {
                     (None,None,false)
                 }
             }
        }
        else {
            (None,None,true)
        }
}

async fn url_to_text(client: &Client, url: &str) -> Result<String, Error> {
    let response = client.get(url).send().await?;
    response.text().await
}

fn process_crawled(
    response: &ScrapEntry,
    lang_detector: &Detector,
    accept_langs: &[Lang],
    accept_all: bool,
) -> (WetRecord, Option<Vec<CrawlEntry>>) {
    let soup = response.response.to_soup();
    let wet_record = response.response.to_warcrecord(Some(&soup));
    let outlinks = if response.crawl_depth != 0
        && (accept_all || has_language(lang_detector, &wet_record.body, accept_langs))
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
