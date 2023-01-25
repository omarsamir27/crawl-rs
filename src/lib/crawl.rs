use std::collections::{HashSet, VecDeque};
use std::fmt::{format, Debug};
use std::fs::File;
use std::io::{BufWriter, Read};
use std::net::IpAddr;
use std::os::linux::raw::stat;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{channel, Receiver, sync_channel};
use std::time::Duration;

use futures::future::join_all;
use futures::FutureExt;
use libflate::{finish, gzip};
use libflate::gzip::Encoder;
// use flate2::write::GzEncoder;
use num_cpus;
use rayon::prelude::*;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{StatusCode, Version};
use soup::Soup;
use tokio::task::JoinHandle;
extern crate warc;
use warc::{BufferedBody, RawRecordHeader, Record, RecordType, WarcHeader, WarcWriter};
use whatlang::{Detector,Lang};

use crate::lang::has_language;
use crate::response::{Response, ResponseError, WetRecord};
use crate::{crawl_utils, lang};

type CrawlQueue = Arc<std::sync::Mutex<Vec<CrawlEntry>>>;
type ScrapQueue = Arc<std::sync::Mutex<Vec<ScrapEntry>>>;
type WetFile = Arc<WarcWriter<std::io::BufWriter<Encoder<std::fs::File>>>>;

struct CrawlEntry {
    url: String,
    crawl_depth: u8,
}

struct ScrapEntry {
    pub response: Response,
    pub crawl_depth: u8,
}

pub fn crawl(seedlist: Vec<String>, initial_depth: u8) {
    let crawl_entries: Vec<CrawlEntry> = seedlist
        .into_iter()
        .map(|url| CrawlEntry {
            url,
            crawl_depth: initial_depth,
        })
        .collect();
    let (tx_bgwriter,rx_bgwriter) = channel();
    let (tx_url,rx_url) = crossbeam::channel::unbounded();
    let (tx_crawled,rx_crawled) = crossbeam::channel::unbounded();
    let url_queue: CrawlQueue = Arc::new(Mutex::new(Vec::from(crawl_entries)));
    let page_queue: ScrapQueue = Arc::new(Mutex::new(Vec::new()));
    let visited_map = Arc::new(RwLock::new(HashSet::new()));
    let num_visited = Arc::new(AtomicU32::new(0));
    let num_bad = Arc::new(AtomicU32::new(0));

    let mut wet_file = WarcWriter::from_path_gzip("warc_0000.warc.wet.gz").unwrap();
    let mut processed_counter = Arc::new(AtomicU32::new(0));
    let model_langs = vec!["arabic", "english"];
    let lang_detector = lang::build_langdetector(model_langs);
    // let accept_langs = vec![Lang::Arabic];
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.spawn_blocking(move ||
        {
            background_writer(rx,wet_file)
        }
    );

    while !url_queue.lock().unwrap().is_empty() || !page_queue.lock().unwrap().is_empty() {
        println!("urlQ: {}", url_queue.lock().unwrap().len());
        println!("pageQ: {}", page_queue.lock().unwrap().len());
        let mut futures = Vec::with_capacity(10);
        for _ in 0..100 {
            let urlQ = url_queue.clone();
            let pageQ = page_queue.clone();
            let visited = num_visited.clone();
            let bad = num_bad.clone();
            let visited_set = visited_map.clone();
            let future = async move { crawl_url(urlQ, pageQ, visited, bad,10,visited_set).await };
            futures.push(future)
        }

        rt.block_on(join_all(futures));
        println!("WAS ABLE TO CRAWL {}", num_visited.load(Ordering::Relaxed));
        println!("BAD LINK {}", num_bad.load(Ordering::Relaxed));

        page_queue
            .lock()
            .unwrap()
            .par_iter()
            .map(|response| {
                process_page(
                    response,
                    &lang_detector,
                    Lang::Ara,
                    processed_counter.clone(),
                    url_queue.clone()
                )
            }).for_each_with(tx.clone(),|tx,record| tx.send(Some(record)).unwrap());
        println!("FINISHED PROCESSING");
        // for mut entry in wetQ {
        //     wet_file.write_raw(entry.headers, &entry.body).unwrap();
        // }
        page_queue.lock().unwrap().clear();
    }
    tx.send(None).unwrap();

}

async fn crawl_url(
    urls_queue: CrawlQueue,
    page_queue: ScrapQueue,
    visited: Arc<AtomicU32>,
    num_bad: Arc<AtomicU32>,
    url_limit: i32,
    visited_set: Arc<RwLock<HashSet<String>>>,
) {
    let client: reqwest::Client = reqwest::Client::new();
    let state = urls_queue.lock().unwrap().is_empty();
    // println!("empty:{}",state);
    let mut limit = 0 ;
    while !state && limit != url_limit {
        // println!("here");
        let crawl_entry = urls_queue.lock().unwrap().pop();
        if crawl_entry.is_none() {
            break;
        }
        let crawl_entry = crawl_entry.unwrap();
        if visited_set.read().unwrap().contains(crawl_entry.url.as_str()) { continue }
        let resp = client.get(crawl_entry.url.to_owned()).timeout(Duration::from_secs(3)).send().await;
        visited_set.write().unwrap().insert(crawl_entry.url.clone());
        if resp.is_err() {
            num_bad.fetch_add(1,Relaxed);
            continue;
        }
        let resp = resp.unwrap();
        let response = Response::from_request(resp).await;
        let response = match response {
            Ok(resp) => {resp }
            Err(_) => {
                num_bad.fetch_add(1,Relaxed);
                continue;
            }
        };
        let scrap_entry = ScrapEntry {
            response,
            crawl_depth: crawl_entry.crawl_depth - 1,
        };
        page_queue.lock().unwrap().push(scrap_entry);
        visited.fetch_add(1, Ordering::Relaxed);

        println!("{}", visited.load(Ordering::Relaxed));
        limit +=1;
    }
}

fn process_page(
    response: &ScrapEntry,
    lang_detector: &Detector,
    accept_langs: Lang,
    processed: Arc<AtomicU32>,
    url_queue: CrawlQueue
) -> WetRecord {
    let soup = response.response.to_soup();
    let wet_record = response.response.to_warcrecord(Some(&soup));
    processed.fetch_add(1,Relaxed);
    if response.crawl_depth != 0 {
        let has_target_langs = has_language(lang_detector, &wet_record.body, accept_langs,4);
        if has_target_langs {
            let outlinks = crawl_utils::soup_links(&soup, &[]);
            if !outlinks.is_empty() {
                let mut outlinks: Vec<CrawlEntry> = outlinks
                    .into_iter()
                    .map(|link| CrawlEntry {
                        url: link,
                        crawl_depth: response.crawl_depth,
                    })
                    .collect();

                url_queue.lock().unwrap().append(&mut outlinks);
            }
        }
    }
    wet_record
}

fn background_writer(records: Receiver<Option<WetRecord>>, mut warc: WarcWriter<BufWriter<Encoder<File>>>){
    loop {
        if let Ok(rec) = records.recv(){
            if let Some(rec) = rec{
                warc.write_raw(rec.headers,&rec.body).unwrap();
            }
            else {
                break
            }
        }
    }
    unsafe {
        let gzip_stream = warc.into_inner().unwrap_unchecked();
        gzip_stream.finish().unwrap();
    }
}
