use crate::error::WclError;
use std::collections::HashMap;
use std::sync::Arc;
use texting_robots::Robot;
use tokio::sync::RwLock;
use url::Url;

const AGENT: &str = "wcl";

use std::time::Instant;
use tokio::sync::Mutex;

struct HostRules {
    robot: Option<Robot>,
    last_request: Mutex<Option<Instant>>,
}

/// Per-host robots.txt cache and politeness delay tracker.
#[derive(Clone)]
pub struct Politeness {
    client: reqwest::Client,
    ignore_robots: bool,
    no_delay: bool,
    hosts: Arc<RwLock<HashMap<String, Arc<HostRules>>>>,
}

impl Politeness {
    pub fn new(client: reqwest::Client, ignore_robots: bool, no_delay: bool) -> Self {
        Self {
            client,
            ignore_robots,
            no_delay,
            hosts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn host_key(url: &Url) -> String {
        format!("{}://{}", url.scheme(), url.authority())
    }

    async fn rules_for(&self, url: &Url) -> Arc<HostRules> {
        let key = Self::host_key(url);
        {
            let map = self.hosts.read().await;
            if let Some(r) = map.get(&key) {
                return Arc::clone(r);
            }
        }

        let robots_url = format!("{key}/robots.txt");
        let body = match self.client.get(&robots_url).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            // A missing or erroring robots.txt means "no restrictions" per RFC 9309.
            _ => String::new(),
        };
        let robot = Robot::new(AGENT, body.as_bytes()).ok();

        let rules = Arc::new(HostRules {
            robot,
            last_request: Mutex::new(None),
        });
        let mut map = self.hosts.write().await;
        Arc::clone(map.entry(key).or_insert(rules))
    }

    pub async fn check(&self, url: &Url) -> Result<(), WclError> {
        if self.ignore_robots {
            return Ok(());
        }
        let rules = self.rules_for(url).await;
        match &rules.robot {
            Some(r) if !r.allowed(url.as_str()) => Err(WclError::Robots(url.to_string())),
            _ => Ok(()),
        }
    }

    pub async fn wait(&self, url: &Url) {
        if self.ignore_robots || self.no_delay {
            return;
        }
        let rules = self.rules_for(url).await;
        if let Some(robot) = &rules.robot {
            if let Some(delay_secs) = robot.delay {
                if delay_secs > 0.0 {
                    let mut last = rules.last_request.lock().await;
                    let target_delay = std::time::Duration::from_secs_f32(delay_secs.min(10.0));
                    if let Some(prev) = *last {
                        let elapsed = prev.elapsed();
                        if elapsed < target_delay {
                            tokio::time::sleep(target_delay - elapsed).await;
                        }
                    }
                    *last = Some(Instant::now());
                }
            }
        }
    }

    pub async fn sitemaps_for(&self, url: &Url) -> Vec<String> {
        let rules = self.rules_for(url).await;
        rules
            .robot
            .as_ref()
            .map(|r| r.sitemaps.clone())
            .unwrap_or_default()
    }
}
