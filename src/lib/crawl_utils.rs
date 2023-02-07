use crate::robots::Robots;
use crate::CrawlEntry;
use ahash::AHashMap;
use itertools::Itertools;
use reqwest::header::HeaderMap;
use soup;
use soup::{NodeExt, QueryBuilderExt, Soup};
use std::collections::hash_map::Entry;
use std::fmt::Display;
use std::fs;
use std::hash::Hash;
use std::path::Path;
use url::Url;

pub fn http_headers_fmt(header_map: &HeaderMap) -> String {
    let mut displayed = String::new();
    for (k, v) in header_map {
        displayed.push_str(k.to_owned().as_ref());
        displayed.push_str(": ");
        displayed.push_str(v.to_str().unwrap_or_default());
        displayed.push_str("\r\n");
    }
    displayed.pop();
    displayed.pop();
    displayed
}

pub fn array_stringify<T: Display>(arr: &[T], delim: char) -> String {
    let mut string = String::new();
    for elem in arr {
        string.push_str(elem.to_string().as_str());
        string.push(delim);
    }
    string.pop();
    string
}

pub fn file_lines(file: &Path) -> Vec<String> {
    fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

pub fn soup_links(soup: &Soup, protocols: &[String]) -> Vec<String> {
    let mut links = vec![];
    for link in soup.tag("a").find_all() {
        if let Some(href) = link.get("href") {
            if !protocols.is_empty() {
                for protocol in protocols {
                    if href.starts_with(protocol) {
                        links.push(href);
                        break;
                    }
                }
            } else {
                links.push(href);
            }
        }
    }
    links
}

#[inline]
pub fn soup_text(soup: &Soup) -> String {
    soup.text()
}

pub fn init_seed_list(file: &Path, recursion_depth: u8) -> Vec<CrawlEntry> {
    let initial = file_lines(file)
        .into_iter()
        .unique()
        .filter(|url| Url::parse(url).is_ok())
        .map(|url| CrawlEntry::new(url, recursion_depth))
        .collect();
    disperse_domains(initial)
}

#[inline]
fn push_or_insert<K, V>(map: &mut AHashMap<K, Vec<V>>, key: K, value: V)
where
    K: Hash + Eq + Clone,
{
    let entry = map.entry(key);
    match entry {
        Entry::Occupied(_) => {
            entry.and_modify(|v| v.push(value));
        }
        Entry::Vacant(_) => {
            entry.or_insert(vec![value]);
        }
    };
}

#[inline]
pub fn disperse_domains(links: Vec<CrawlEntry>) -> Vec<CrawlEntry> {
    let capacity = links.capacity();
    let count = links.len();
    let mut round_map: AHashMap<String, Vec<CrawlEntry>> = ahash::AHashMap::new();
    for link in links {
        let domain = Robots::extract_domain(&link.url).unwrap();
        push_or_insert(&mut round_map, domain, link);
    }
    let mut buckets: Vec<Vec<CrawlEntry>> = round_map.into_values().collect();
    let mut results = Vec::with_capacity(capacity);
    let bucket_count = buckets.len();
    for bucket in (0..bucket_count).cycle() {
        if let Some(url) = buckets[bucket].pop() {
            results.push(url);
        }
        if results.len() == count {
            break;
        }
    }
    results
}
