use config::{Config, ConfigError};
use libcrawl::{ job_config,crawl, crawl_utils};
use std::collections::HashMap;
use std::process::exit;

// mod job_config;
// use job_config::hi;

const src: &str = "0000 (copy).warc_urls";
const dst: &str = "reswarcs.warc.wet.gz";

fn main() {
    let cmd_args: Vec<String> = std::env::args().collect();
    if cmd_args.len() != 2 {
        eprintln!("Wrong number of arguments: must supply only 1 argument");
        exit(1)
    }
    let job = match job_config::read_job_config(&cmd_args[1]) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", e.to_string());
            exit(1)
        }
    };
    if let Some(errors) = job_config::check_config(&job){
        for e in errors{
            eprintln!("{e}");
        }
        exit(1)
    }

    // crawl::start_crawl(urls,dst,500);
}
