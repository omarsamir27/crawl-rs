use chashmap_async::CHashMap;
use texting_robots::{get_robots_url, Robot};
use tokio::time::Instant;
use url::{ParseError, Url};

pub struct Robots {
    permissions: CHashMap<Url, Rules>,
}
impl Robots {
    pub fn new() -> Self {
        Self {
            permissions: CHashMap::new(),
        }
    }
    pub fn valid_url(url: &str) -> Result<Url, ParseError> {
        Url::parse(url)
    }
    pub fn to_domain(url: &Url) -> Option<Url> {
        let scheme = url.scheme();
        let domain = url.host_str()?;
        let domain = format!("{scheme}://{domain}");
        Self::valid_url(domain.as_str()).ok()
    }
    pub fn extract_domain(url: &str) -> Option<String> {
        let url = Self::valid_url(url).ok()?;
        url.host_str().map(|dom| dom.to_string())
    }

    pub async fn has_domain(&self, url: &Url) -> bool {
        self.permissions.contains_key(url).await
    }
    pub async fn can_visit_url(&self, url: &Url, domain: &Url) -> RobotsVerdict {
        let rules = self.permissions.get(domain).await.unwrap();
        rules.can_visit_path(url.as_str())
    }
    pub fn robots_url(domain: &Url) -> String {
        get_robots_url(domain.as_str()).unwrap()
    }
    pub async fn insert_entry(&self, domain: &Url, txt: &str) -> bool {
        let rule = match Rules::new("*", txt.as_bytes()) {
            None => return false,
            Some(rule) => rule,
        };
        self.permissions.insert_new(domain.clone(), rule).await;
        true
    }
    pub async fn update_domain(&self, domain: &Url) {
        self.permissions.get_mut(domain).await.unwrap().last_visited = Some(Instant::now())
    }
}

struct Rules {
    rules: Robot,
    last_visited: Option<Instant>,
}

impl Rules {
    fn new(agent: &str, robots: &[u8]) -> Option<Rules> {
        Robot::new(agent, robots).map_or(None, |rules| {
            Some(Rules {
                rules,
                last_visited: None,
            })
        })
    }
    pub fn can_visit_path(&self, path: &str) -> RobotsVerdict {
        let cooldown_period = self.rules.delay;
        let delay_passed = cooldown_period.is_none()
            || self.last_visited.is_none()
            || (self.last_visited.as_ref().unwrap().elapsed().as_secs_f32()
                >= cooldown_period.unwrap());
        RobotsVerdict::new(self.rules.allowed(path), delay_passed)
    }
}

#[derive(Debug)]
pub enum RobotsVerdict {
    ForbiddenPath,
    CrawlDelay,
    Proceed,
}
impl RobotsVerdict {
    pub fn new(path_allowed: bool, delay_passed: bool) -> Self {
        match (path_allowed, delay_passed) {
            (false, _) => Self::ForbiddenPath,
            (true, false) => Self::CrawlDelay,
            (true, true) => Self::Proceed,
        }
    }
}
