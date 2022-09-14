use futures::future::join_all;
use isahc::ResponseExt;
use libcrawl::crawl_utils;
use std::io;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

static mut VISITED: AtomicU32 = AtomicU32::new(0);

#[tokio::main(flavor = "multi_thread", worker_threads = 6)]
async fn main() -> io::Result<()> {
    let urls = crawl_utils::file_lines("0000.warc_urls");
    let Q: Arc<Mutex<Vec<String>>> = Arc::new(tokio::sync::Mutex::new(vec![]));
    for url in urls {
        unsafe {
            Q.lock().await.push(url);
        }
    }
    let mut futures = vec![];
    for _ in 0..=10 {
        let q = Q.clone();
        futures.push(tokio::spawn(async move { unsafe { visit(q).await } }))
    }
    println!("scheded");
    join_all(futures).await;

    Ok(())
}

async unsafe fn visit(Q: Arc<Mutex<Vec<String>>>) {
    let client: reqwest::Client = reqwest::ClientBuilder::new().gzip(false).build().unwrap();
    // let client = isahc::HttpClient::new().unwrap();
    while Q.lock().await.is_empty() == false {
        let url = Q.lock().await.pop().unwrap();
        let resp = client.get(url).send().await;
        // let resp = client.get_async(url).await;
        if resp.is_err() {
            continue;
        }
        let resp = resp.unwrap();
        println!("{}", resp.url());
        println!("{:?}", resp.headers());
        // println!("{}", crawl_utils::http_headers_fmt(resp.headers()));
        // let text = resp.text().await;
        // if text.is_err() {continue;}
        // let text= text.unwrap();
        // println!("{}",text);
        unsafe {
            VISITED.fetch_add(1, Ordering::Relaxed);
        }
        println!("{}", VISITED.load(Ordering::Relaxed));
    }
}
