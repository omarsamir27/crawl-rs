use config::Config;
use libcrawl::{crawl, crawl_utils, job_config};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let job = match parse_config() {
        None => return ExitCode::FAILURE,
        Some(job) => job,
    };
    let seeds_file = job.get_string("seeds").unwrap();
    if !Path::is_file(Path::new(seeds_file.as_str())) {
        eprintln!("Seeds file {seeds_file} does not exist");
        return ExitCode::FAILURE;
    }
    let seeds = crawl_utils::init_seed_list(
        seeds_file.as_ref(),
        job.get_int("crawl_recursion").unwrap() as u8,
    );
    crawl::start_crawl(seeds, &job);
    ExitCode::SUCCESS
}

fn parse_config() -> Option<Config> {
    let cmd_args: Vec<String> = std::env::args().collect();
    if cmd_args.len() != 2 {
        eprintln!("Wrong number of arguments: must supply only 1 argument");
        return None;
    }
    let config_builder = job_config::default_config();
    let job = match job_config::read_job_config(&cmd_args[1], config_builder) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    if let Some(errors) = job_config::check_config(&job) {
        for e in errors {
            eprintln!("{e}");
        }
        return None;
    }
    Some(job)
}
