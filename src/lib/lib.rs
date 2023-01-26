#![feature(async_closure)]

extern crate core;

// pub mod crawl;
pub mod crawl_utils;
mod lang;
pub mod response;
pub mod crawl2;

pub struct CrawlEntry {
    pub url: String,
    pub crawl_depth: u8,
}

pub struct ScrapEntry {
    pub response: crate::response::Response,
    pub crawl_depth: u8,
}
