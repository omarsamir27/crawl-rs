use std::cmp::min;
use std::collections::HashSet;
use std::io::BufRead;
use whatlang::{Detector, Lang,};

#[inline(always)]
fn str_to_lang(lang: &str) -> Option<Lang> {
    let lang = lang.to_lowercase();
    match lang.as_str() {
        "arabic" | "ar" => Some(Lang::Ara),
        "english" | "en" => Some(Lang::Eng),
        "french" | "fr" => Some(Lang::Fra),
        _ => None,
    }
}

fn lang_builder(langs: Vec<&str>) -> Vec<Lang> {
    let mut detect_langs = Vec::new();
    for lang in langs {
        if let Some(language) = str_to_lang(lang) {
            detect_langs.push(language)
        }
    }
    detect_langs
}

pub fn build_langdetector(langs: Vec<&str>) -> Detector {
    let langs = lang_builder(langs);

    Detector::with_allowlist(langs)
}

pub fn has_language(detector: &Detector, text: &str, language: Lang,detection_granularity:usize) -> bool {
    let text_len = text.len();
    if let Some(lang) = detector.detect_lang(text){
        if lang == language{
            return true;
        }
    };
    let text_piece : Vec<&str> = text.split(" ").collect();
    for txt in text_piece{
        println!("NOT ENOUGH");
        if let Some(lang) = detector.detect_lang( txt )
        {
            if lang == language {
                return true
            }
        }
    }
    false
}
