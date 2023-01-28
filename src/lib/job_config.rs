use std::collections::HashMap;
use config::{Config, ConfigError, Source, Value, ValueKind};
use futures::future::err;
use thiserror::Error;
use crate::job_config::CrawlerConfigError::{MandatoryFieldMissing, WrongFieldType};

#[derive(Error,Debug)]
pub enum CrawlerConfigError{
    #[error("Field `{0}` is mandatory but missing")]
    MandatoryFieldMissing(String),
    #[error("Wrong field type, expected `{0}`")]
    WrongFieldType(String)
}

pub fn read_job_config(job:&str) -> Result<Config, ConfigError> {
    Config::builder().add_source(config::File::with_name(job)).build()
}

pub fn check_config(config:&Config) -> Option<Vec<CrawlerConfigError>> {
    let mandatory_fields = ["seeds",];
    let type_checks = HashMap::from(
        [
            ("seeds","bool"),
            ("destination_warc","string"),
            ("link_timeout","uint"),
            ("crawl_tasks","uint"),
            ("crawl_recursion","uint"),
            ("accept_languages","vec<string>")
        ]
    );
    let mut errors = Vec::new();
    let map = config.collect().unwrap();
    for field in mandatory_fields{
        if !map.contains_key(field){
            errors.push(MandatoryFieldMissing(field.to_string()))
        }
    }
    for (k,v) in &map{
        let Type = *type_checks.get(k.as_str()).unwrap();
        match Type {
            "bool" if v.clone().into_bool().is_err() => errors.push(WrongFieldType(Type.to_string())),
            "string" if v.clone().into_string().is_err() => errors.push(WrongFieldType(Type.to_string())),
            "uint" if v.clone().into_uint().is_err() => errors.push(WrongFieldType(Type.to_string())),
            "vec<string>" => {
                if let Ok(array) = v.clone().into_array(){
                    if array.iter().any(|string| string.clone().into_string().is_err()){
                        errors.push(WrongFieldType(Type.to_string()))
                    }
                }
                else {
                    errors.push(WrongFieldType(Type.to_string()))
                }
            }
            _ => unreachable!()
        }
    }
    if errors.is_empty(){
        None
    }
    else { Some(errors) }
}