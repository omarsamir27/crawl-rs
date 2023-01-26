use libcrawl::{crawl2, crawl_utils, CrawlEntry};
use whatlang::{Detector,Lang};
use soup::Soup;

const src : &str = "0000.warc_urls";
const dst : &str = "reswarcs.warc.wet.gz";

fn main() {
    let urls = crawl_utils::file_lines(src).into_iter()
        .map(|url| CrawlEntry {
            url,
            crawl_depth: 2,
        })
        .collect();
    crawl2::start_crawl(urls,dst,1000);
    // crawl::crawl(urls, 2);
    // // // let text = include_str!("402081.html");
    // // // let soup = Soup::new(text);
    // // // let txt = soup.text();
    // let det = whatlang::Detector::with_allowlist(vec![Lang::Ara,Lang::Eng]);
    // // let en = whatlang::Detector::with_allowlist(vec![Lang::Eng]);
    //
    // // let text = "i am عمر عبيط بشدة  عزيز very ضرب very سعيد very" ;
    // let text = "omar samir is my name , اسمي عمر سمير";
    // let res1 = det.detect_lang(&text[..text.len() / 2]);
    // let res2 = det.detect_lang(&text[text.len() / 2..]);
    // println!("{:?}----------{:?}",res1,res2);
}
