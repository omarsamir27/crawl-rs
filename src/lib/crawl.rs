use crate::crawl_utils;
use futures::future::join_all;
use num_cpus;
use reqwest::header::{HeaderMap, HeaderValue};
use soup::Soup;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

type CrawlQueue = Arc<tokio::sync::Mutex<VecDeque<CrawlEntry>>>;
type ScrapQueue = Arc<tokio::sync::Mutex<VecDeque<ScrapEntry>>>;

struct Response {
    url: String,
    data: String,
    content_length: u64,
    headers: String,
    time: String,
}

impl Response {
    fn new(url: &str, data: &str, content_length: u64, headers: &HeaderMap, time: &str) -> Self {
        let mut headers = headers.clone();
        headers.insert("content-length", HeaderValue::from(content_length));
        let headers = crawl_utils::http_headers_fmt(&headers);
        Response {
            url: url.to_string(),
            data: data.to_string(),
            content_length,
            headers,
            time: time.to_string(),
        }
    }
    fn to_soup(&self) -> Soup {
        return Soup::new(self.data.as_str());
    }
}

struct CrawlEntry {
    url: String,
    crawl_depth: u8,
}

struct ScrapEntry {
    pub response: Response,
    pub crawl_depth: u8,
}

pub fn crawl(seedlist: Vec<String>, initial_depth: u8) {
    let num_cores = num_cpus::get_physical() as u32;
    let num_crawlers = (2 / 3) * num_cores;
    let num_scrapers = (1 / 3) * num_cores;
    let crawl_entries: Vec<CrawlEntry> = seedlist
        .into_iter()
        .map(|url| CrawlEntry {
            url,
            crawl_depth: initial_depth,
        })
        .collect();
    let url_queue: CrawlQueue = Arc::new(tokio::sync::Mutex::new(VecDeque::from(crawl_entries)));
    let page_queue: ScrapQueue = Arc::new(tokio::sync::Mutex::new(VecDeque::new()));
    let num_visited = Arc::new(AtomicU32::new(0));
    let num_processed = Arc::new(AtomicU32::new(0));
    let rt = tokio::runtime::Builder::new_multi_thread().build().unwrap();
    let mut futures = vec![];
    for _ in 0..num_crawlers {
        let urlQ = url_queue.clone();
        let pageQ = page_queue.clone();
        let visited = num_visited.clone();
        futures.push(tokio::spawn(async move {
            crawl_url(urlQ, pageQ, visited).await
        }))
    }
    for _ in 0..num_scrapers {
        let urlQ = url_queue.clone();
        let pageQ = page_queue.clone();
        let processed = num_processed.clone();
        futures.push(tokio::spawn(async move {
            scrap_page(urlQ, pageQ, processed).await
        }))
    }

    rt.block_on(join_all(futures));
}

async fn crawl_url(urls_queue: CrawlQueue, page_queue: ScrapQueue, visited: Arc<AtomicU32>) {
    let client: reqwest::Client = reqwest::Client::new();
    while !urls_queue.lock().await.is_empty() {
        let crawl_entry = urls_queue.lock().await.pop_front().unwrap();
        let resp = client.get(crawl_entry.url.to_owned()).send().await;
        if resp.is_err() {
            continue;
        }
        let resp = resp.unwrap();
        let content_length: u64 = match resp.content_length() {
            None => 0,
            Some(length) => length,
        };
        let headers = resp.headers().clone();
        let text = resp.text().await;
        if text.is_err() {
            continue;
        }
        let time = chrono::Local::now().to_string();
        let text = text.unwrap();

        let response = Response::new(
            crawl_entry.url.as_str(),
            text.as_str(),
            content_length,
            &headers,
            time.as_str(),
        );
        let scrap_entry = ScrapEntry {
            response,
            crawl_depth: crawl_entry.crawl_depth - 1,
        };
        page_queue.lock().await.push_back(scrap_entry);
        visited.fetch_add(1, Ordering::Relaxed);

        println!("{}", visited.load(Ordering::Relaxed));
    }
}

async fn scrap_page(urls_queue: CrawlQueue, page_queue: ScrapQueue, processed: Arc<AtomicU32>) {
    while !page_queue.lock().await.is_empty() {
        let scrap_entry = page_queue.lock().await.pop_front().unwrap();
        let (text, links) = {
            let soup = scrap_entry.response.to_soup();
            let text = soup.text();
            if scrap_entry.crawl_depth != 0 {
                (text, Some(crawl_utils::soup_links(&soup, &[])))
            } else {
                (text, None)
            }
        };
        if links.is_some() {
            let links: Vec<CrawlEntry> = links
                .unwrap()
                .into_iter()
                .map(|link| CrawlEntry {
                    url: link,
                    crawl_depth: scrap_entry.crawl_depth,
                })
                .collect();
            urls_queue.lock().await.extend(links);
        }
    }
}
