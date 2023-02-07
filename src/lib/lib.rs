#![feature(let_chains)]

extern crate core;

pub mod crawl;
pub mod crawl_utils;
pub mod job_config;
mod lang;
pub mod response;
pub mod robots;
// pub mod crawl2;

pub struct CrawlEntry {
    pub url: String,
    pub crawl_depth: u8,
}

impl CrawlEntry {
    pub fn new(url: String, crawl_depth: u8) -> Self {
        Self { url, crawl_depth }
    }
}

pub struct ScrapEntry {
    pub response: crate::response::Response,
    pub crawl_depth: u8,
}
