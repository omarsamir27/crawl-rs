use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel as std_channel, Receiver, Sender};
use std::thread::sleep;
use std::time::Duration;

use ahash::AHashSet;
use async_channel::Receiver as AsyncReceiver;
use async_channel::Sender as AsyncSender;
use colored::Colorize;
use config::Config;
use futures::future::join_all;
use libflate::gzip::Encoder;
use reqwest::{Client, Error};
use url::Url;
use warc::WarcWriter;
use whatlang::{Detector, Lang};

use crate::{crawl_utils, CrawlCounters, CrawlEntry, lang, ScrapEntry};
use crate::crawl_utils::disperse_domains;
use crate::lang::has_language;
use crate::response::{Response, WetRecord};
use crate::robots::{Robots, RobotsVerdict};

type WetFile = WarcWriter<BufWriter<Encoder<File>>>;

pub fn start_crawl(seeds: Vec<CrawlEntry>, job: &Config) {
    let (warc_dst, crawler_count, link_timeout, accept_langs, respect_robots) = (
        job.get_string("destination_warc").unwrap(),
        job.get_int("crawl_tasks").unwrap() as usize,
        job.get_int("link_timeout").unwrap() as u64,
        job.get_array("accept_languages")
            .unwrap()
            .into_iter()
            .map(|value| value.into_string().unwrap())
            .collect::<Vec<String>>(),
        job.get_bool("respect_robots").unwrap(),
    );
    let wet_file = WarcWriter::from_path_gzip(&warc_dst).unwrap();
    let bad_urls_log = BufWriter::new(
        File::options()
            .read(true)
            .append(true)
            .create(true)
            .open(&format!("{warc_dst}.LOG"))
            .unwrap(),
    );
    let (tx_processor_writer, rx_bgwriter) = std_channel();
    let (tx_processor, rx_crawler) = async_channel::unbounded();
    let (tx_crawler, rx_processor) = std_channel::<ScrapEntry>();
    let (tx_crawl_log,rx_logger) = std_channel::<String>();
    let model_langs = vec!["arabic", "english"];
    let accept_langs = lang::lang_builder(accept_langs.iter().map(|lang| lang.as_str()).collect());
    let lang_detector = lang::build_langdetector(model_langs);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let counters = Arc::new(CrawlCounters::default());
    counters.add_to("queued", seeds.len() as u64);
    let mut known_urls: AHashSet<String> = seeds.iter().map(|entry| entry.url.clone()).collect();
    for x in seeds {
        tx_processor.send_blocking(x).unwrap();
    }
    let started_crawling = Arc::new(AtomicBool::new(false));
    let mut crawlers = Vec::with_capacity(crawler_count);
    let client: reqwest::Client = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(link_timeout))
        .build()
        .unwrap();
    let robots = Arc::new(Robots::new());
    for _ in 0..crawler_count {
        crawlers.push(crawl_url(
            client.clone(),
            rx_crawler.clone(),
            tx_crawler.clone(),
            counters.clone(),
            started_crawling.clone(),
            robots.clone(),
            tx_processor.clone(),
            respect_robots,
            tx_crawl_log.clone()
        ));
    }
    let accept_all = accept_langs.is_empty();
    let counters2 = counters.clone();
    rt.spawn_blocking(move || {
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
                        links.retain(|i| {
                            url::Url::parse(&(i.url)).is_ok_and(|url| !url.cannot_be_a_base() && ["http", "https"].contains(&url.scheme()))
                                && known_urls.insert(i.url.clone())
                        });
                        if tx_processor.is_empty() || link_cache.len() >= 400 {
                            let cached = mem::take(&mut link_cache);
                            let dispersed = disperse_domains(cached);
                            counters2.add_to("extra", dispersed.len() as u64);
                            counters2.add_to("queued", dispersed.len() as u64);
                            for x in dispersed {
                                tx_processor.send_blocking(x).unwrap()
                            }
                        } else {
                            link_cache.append(&mut links)
                        }
                    }
                }
                Err(_) => {
                    if tx_processor.is_empty() {
                        break;
                    } else {
                        continue;
                    }
                }
            }
        }
    });
    rt.spawn_blocking(|| background_writer(rx_bgwriter, wet_file));
    rt.spawn_blocking(|| log(rx_logger,bad_urls_log));
    let counters3 = counters.clone();
    rt.spawn(async move {
        loop {
            println!("{}", counters3.current_ongoing().green());
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    rt.block_on(join_all(crawlers));
    println!("{}", counters.to_string().blue());
}

async fn crawl_url(
    client: Client,
    rx_url: AsyncReceiver<CrawlEntry>,
    tx_page: Sender<ScrapEntry>,
    counters: Arc<CrawlCounters>,
    started_crawling: Arc<AtomicBool>,
    robots: Arc<Robots>,
    loopback: AsyncSender<CrawlEntry>,
    respect_robots: bool,
    tx_crawl_log : Sender<String>
) {
    started_crawling.store(true, Ordering::Relaxed);
    while let Ok(crawl_entry) = rx_url.recv().await {
        counters.decrement_queued();
        /*
        ask if url is valid,
        if yes : ask if domain robots has been saved , if yes : ask if can visit again
        else : add domain rules , then ask if can visit
         */
        if respect_robots {
            let (verdict, domain, malformed_url) =
                eval_robots(&client, &crawl_entry.url, &robots).await;
            match verdict {
                None if malformed_url => continue,
                None if !malformed_url => {}
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
        }
        let resp = client.get(&crawl_entry.url).send().await;
        let response = match resp {
            Ok(resp) => match Response::from_request(resp).await {
                Ok(resp) => Some(resp),
                Err(e) => {
                    eprintln!("{}", format!("{} failed", &crawl_entry.url).red());
                    None
                }
            },
            Err(e) => {
                eprintln!("{}", format!("{} failed", &crawl_entry.url).red());
                None
            }
        };

        if let Some(response) = response {
            let scrap_entry = ScrapEntry {
                response,
                crawl_depth: crawl_entry.crawl_depth - 1,
            };
            if let Err(e) = tx_page.send(scrap_entry) {
                eprintln!("Sending Error : {e:?}");
            }
            counters.increment_visited();
        } else {
            counters.increment_failed();
            tx_crawl_log.send(crawl_entry.url).unwrap();
        }
    }
}

#[inline(always)]
async fn eval_robots(
    client: &Client,
    url: &str,
    robots: &Arc<Robots>,
) -> (Option<RobotsVerdict>, Option<Url>, bool) {
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
fn background_writer(records: Receiver<WetRecord>, mut warc: WetFile) {
    while let Ok(rec) = records.recv() {
        warc.write_raw(rec.headers, &rec.body).unwrap();
    }
    println!("finish");
    unsafe {
        let gzip_stream = warc.into_inner().unwrap_unchecked();
        gzip_stream.finish().unwrap();
    }
}

fn log(rx:Receiver<String>, mut log:BufWriter<File>){
    let mut cycle = (0..5).cycle();
    while let Ok(url) = rx.recv()  {
        log.write_fmt(format_args!("{url}\n")).unwrap();
        if cycle.next() == Some(4){
            log.flush().unwrap();
        }
    }
    log.flush().unwrap();
}
