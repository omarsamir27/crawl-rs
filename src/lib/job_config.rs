use crate::job_config::CrawlerConfigError::{MandatoryFieldMissing, WrongFieldType};
use config::builder::{BuilderState, DefaultState};
use config::{Config, ConfigBuilder, ConfigError, Map, Source, Value, ValueKind};
use phf::phf_map;
use std::collections::HashMap;
use thiserror::Error;

static TYPE_CHECKS: phf::Map<&'static str, &'static str> = phf_map! {
            "seeds"=>"string",
            "destination_warc"=>"string",
            "link_timeout"=>"uint",
            "crawl_tasks"=>"uint",
            "crawl_recursion"=>"uint",
            "accept_languages"=>"vec<string>"
};

static CONFIG_DEFAULTS: phf::Map<&'static str, ValueKind> = phf_map! {
            "crawl_tasks" => ValueKind::U64(50),
            "link_timeout" => ValueKind::U64(5000),
            "crawl_recursion" => ValueKind::U64(2),
            /*
              destination_warc and accept_languages should be defined in using function
            */
};

#[derive(Error, Debug)]
pub enum CrawlerConfigError {
    #[error("Field `{0}` is mandatory but missing")]
    MandatoryFieldMissing(String),
    #[error("Wrong field type:`{0}`, expected `{1}`")]
    WrongFieldType(String, String),
}

pub fn read_job_config(job: &str,config_builder:ConfigBuilder<DefaultState>) -> Result<Config, ConfigError> {
    config_builder
        .add_source(config::File::with_name(job))
        .build()
}

pub fn check_config(config: &Config) -> Option<Vec<CrawlerConfigError>> {
    let mandatory_fields = ["seeds"];
    let mut errors = Vec::new();
    let map = config.collect().unwrap();
    for field in mandatory_fields {
        if !map.contains_key(field) {
            errors.push(MandatoryFieldMissing(field.to_string()))
        }
    }
    println!("{:?}",map);
    for (k, v) in &map {
        let Type = *TYPE_CHECKS.get(k.as_str()).unwrap();
        println!("{}\t\t{}\t\t{}",k,v,Type);
        match Type {
            "bool" if v.clone().into_bool().is_err() => {
                errors.push(WrongFieldType(k.to_string(), Type.to_string()))
            },
            "string" if v.clone().into_string().is_err() => {
                errors.push(WrongFieldType(k.to_string(), Type.to_string()))
            },
            "uint" if v.clone().into_uint().is_err() => {
                errors.push(WrongFieldType(k.to_string(), Type.to_string()))
            },
            "vec<string>" => {
                if let Ok(array) = v.clone().into_array() {
                    if array
                        .iter()
                        .any(|string| string.clone().into_string().is_err())
                    {
                        errors.push(WrongFieldType(k.to_string(), Type.to_string()))
                    }
                } else {
                    errors.push(WrongFieldType(k.to_string(), Type.to_string()))
                }
            },
            _ => continue
        }
    }
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

pub fn default_config() -> ConfigBuilder<DefaultState> {
    let accept_languages: Vec<String> = Vec::new();
     Config::builder()
        .set_default("crawl_tasks", 20)
        .unwrap()
        .set_default("link_timeout", 5000)
        .unwrap()
        .set_default("crawl_recursion",2)
        .unwrap()
        .set_default("destination_warc",chrono::offset::Local::now().to_rfc3339())
        .unwrap()
        .set_default("accept_languages",accept_languages)
        .unwrap()
}
