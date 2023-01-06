use libcrawl::{crawl, crawl_utils};
use whatlang::{Detector,Lang};
use soup::Soup;

fn main() {
    let urls = crawl_utils::file_lines("0000.warc_urls");
    crawl::crawl(urls, 2);
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
