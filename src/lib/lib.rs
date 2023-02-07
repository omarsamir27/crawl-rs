#![feature(let_chains)]
#![feature(is_some_and)]
#![feature(option_result_contains)]

extern crate core;

use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod crawl;
pub mod crawl_utils;
pub mod job_config;
mod lang;
pub mod response;
pub mod robots;

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
    pub response: response::Response,
    pub crawl_depth: u8,
}

#[derive(Default)]
pub struct CrawlCounters {
    visited: AtomicU64,
    failed: AtomicU64,
    initial: AtomicU64,
    extra: AtomicU64,
    queued: AtomicU64,
}

impl CrawlCounters {
    pub fn new(visited: u64, failed: u64, initial: u64, extra: u64, queued: u64) -> Self {
        Self {
            visited: visited.into(),
            failed: failed.into(),
            initial: initial.into(),
            extra: extra.into(),
            queued: queued.into(),
        }
    }
    pub fn increment_visited(&self) {
        self.visited.fetch_add(1, Ordering::Relaxed);
    }
    pub fn increment_failed(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }
    pub fn increment_queued(&self) {
        self.queued.fetch_add(1, Ordering::Relaxed);
    }
    pub fn decrement_queued(&self){
        self.queued.fetch_sub(1,Ordering::Relaxed);
    }
    pub fn add_to(&self, counter: &str, value: u64) {
        match counter {
            "visited" => self.visited.fetch_add(value, Ordering::Relaxed),
            "failed" => self.failed.fetch_add(value, Ordering::Relaxed),
            "extra" => self.extra.fetch_add(value, Ordering::Relaxed),
            "queued" => self.queued.fetch_add(value, Ordering::Relaxed),
            "initial" => self.initial.fetch_add(value, Ordering::Relaxed),
            _ => 0,
        };
    }
    pub fn current_ongoing(&self) -> String {
        format!(
            "Visited : {}\n\
            Failed : {}\n\
            Extra Extracted : {}\n\
            Links in Queue : {}\n\
            <===========================================================>\n",
            self.visited.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
            self.extra.load(Ordering::Relaxed),
            self.queued.load(Ordering::Relaxed)
        )
    }
}

impl Display for CrawlCounters {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Visited : {}\n\
            Failed : {}\n\
            Extra Extracted : {}\n\
            Links in Queue : {}\n\
            Initial Seeds : {}\n\
            <<<<<<<<<<<<<<<<<<<<TOTAL>>>>>>>>>>>>>>>>>>>>\n",
            self.visited.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
            self.extra.load(Ordering::Relaxed),
            self.queued.load(Ordering::Relaxed),
            self.initial.load(Ordering::Relaxed)
        )
    }
}
