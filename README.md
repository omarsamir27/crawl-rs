# Crawl-RS

## Open source `polite` web scrapper and crawler for web archiving efforts

`Crawl-RS` is a web crawler written in Rust , it uses the `tokio` runtime asynchronous IO to maximize the number of open
connections and uses the **WARC** format to save responses in **WET** files
[WARC Standard](https://iipc.github.io/warc-specifications/specifications/warc-format/warc-1.1/)

### Installing

#### With Cargo

`Crawl-RS` uses **Nightly Rust** , after installing `rustup`

- Install the nightly toolchain by :  
  `$ rustup toolchain install nightly` ,
- Install `Crawl-RS` to your user programs :  
  `$ cargo +nightly install --git https://github.com/omarsamir27/crawl-rs.git`
- The program will be available as `txtcrawl`

### Usage

`$ txtcrawl job.toml`

#### Configuration TOML file format

`txtcrawl` accepts 1 command-line argument : a TOML file , below is the default configuration with the complete
available options , if any optional field is missing , the value here is used

```toml
seeds = ""
crawl_tasks = 20
link_timeout = 5000
crawl_recursion = 2
accept_languages = []
destination_warc = ""
respect_robots = true
```

- **(_mandatory_)** **seeds** : a string with text file path with an initial list of seeds separated by new lines.
- **crawl_tasks** : number of asynchronous workers to use, a value higher than 100 might cause your DNS resolution
  service to stop working on **Linux** if you are using `systemd-resolved`, This is an issue under investigation. If you
  used this `txtcrawl` and your Internet stopped working try `systemctl restart systemd-resolved.service`. This issue
  does not exist on **Windows**
- **link_timeout** : The time in milliseconds a worker waits for connection establishment before marking a url as bad.
- **crawl_recursion** : The breadth of the crawl path from 1 link.
- **accept_languages** : A list of strings that represent languages, any webpage that contains any of `accept_languages`
  is allowed to contribute to the crawl path , current supported values
  are `["ar","arabic","en","english","fr","french"]` , more will be added.
  Note that if a page not containing any of the languages will still be saved if it is present in the seed list, but
  won't further contribute links to crawling. **An empty list means accept all pages**
- **destination_warc** : String containing filename to save output to, the default value is the current time in RFC 3339
  format,
  webpages that fail are logged to a textfile with the same filename suffixed with `.LOG`
- **respect_robots** : Respect **_robots.txt_** of a website if it is available, if **_robots.txt_** is not available,
  the crawler is allowed to visit any path it finds, although it uses a best-effort visiting pattern to not bombard 1
  website repeatedly.

### This work in inspired by `https://github.com/arcalex/txtcrawl` by [mraslann](https://github.com/mraslann)
### This project is my internship work at [Bibliotheca Alexandrina Web Archiving Sector](https://github.com/arcalex) 

### License

#### GPLv3
