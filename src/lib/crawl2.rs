use crate::{CrawlEntry, ScrapEntry};
use async_channel::Receiver as AsyncReceiver;
use futures::future::join_all;
use libflate::gzip::Encoder;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc::{channel as std_channel, Sender, TryRecvError};
use std::sync::{Arc, RwLock};
use tokio::sync::RwLock as AsyncRwLock;
use warc::WarcWriter;

type WetFile = Arc<WarcWriter<std::io::BufWriter<Encoder<std::fs::File>>>>;

fn start_crawl(seeds: Vec<CrawlEntry>, file: &str, crawler_count: usize) {
    let mut wet_file = WarcWriter::from_path_gzip("warc_0000.warc.wet.gz").unwrap();
    let (tx_processor, rx_bgwriter) = std_channel();
    let (tx_processor, rx_crawler) = async_channel::unbounded();
    let (tx_crawler, rx_processor) = std_channel::<ScrapEntry>();
    let visited_urls = Arc::new(AsyncRwLock::new(HashSet::new()));
    let visited_url_count = Arc::new(AtomicU32::new(0));
    let bad_url_count = Arc::new(AtomicU32::new(0));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    // let tx_ld = tx_loader.clone();
    rt.block_on(async {
        for x in seeds {
            tx_processor.send(x).await.unwrap();
        }
    });
    let stall_flags = Arc::new(RwLock::new(vec![false; crawler_count + 1]));
    let mut crawlers = Vec::with_capacity(crawler_count);
    for idx in 0..crawler_count {
        crawlers.push(
            crawl_url(
                idx,
                rx_crawler.clone(),
                tx_crawler.clone(),
                visited_urls.clone(),
                visited_url_count.clone(),
                bad_url_count.clone(),
                stall_flags.clone(),
            )
        );
    }
    rt.spawn_blocking({
        while true {
            match rx_processor.try_recv() {
                Ok(crawled) => {}
                Err(e) => match e {
                    TryRecvError::Empty => {}
                    TryRecvError::Disconnected => {}
                },
            }
        }
    });
    rt.block_on(join_all(crawlers));
}

async fn crawl_url(
    idx: usize,
    rx_url: AsyncReceiver<CrawlEntry>,
    tx_page: Sender<ScrapEntry>,
    visited_urls: Arc<AsyncRwLock<HashSet<String>>>,
    visited_count: Arc<AtomicU32>,
    bad_url_count: Arc<AtomicU32>,
    stall_flags: Arc<RwLock<Vec<bool>>>,
) {
}

fn process_crawled() {}

fn background_writer() {}
