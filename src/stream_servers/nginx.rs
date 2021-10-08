use async_trait::async_trait;
use log::{error, trace};
use serde::{Deserialize, Serialize};

use super::{Bsl, StreamServersCommands, SwitchLogic};
use crate::switcher::{SwitchType, Triggers};

#[derive(Deserialize, Debug)]
struct NginxRtmpStats {
    server: NginxRtmpServer,
}

#[derive(Deserialize, Debug)]
struct NginxRtmpServer {
    application: Vec<NginxRtmpApp>,
}

#[derive(Deserialize, Debug)]
struct NginxRtmpApp {
    name: String,
    live: NginxRtmpLive,
}

#[derive(Deserialize, Debug)]
struct NginxRtmpLive {
    stream: Option<Vec<NginxRtmpStream>>,
}

#[derive(Deserialize, Debug)]
pub struct NginxRtmpStream {
    pub name: String,
    pub bw_video: u32,
    pub meta: Option<Meta>,
}

#[derive(Deserialize, Debug)]
pub struct Meta {
    video: Video,
    audio: Audio,
}

#[derive(Deserialize, Debug)]
pub struct Video {
    width: u32,
    height: u32,
    frame_rate: u32,
    codec: String,
    profile: Option<String>,
    compat: Option<u32>,
    level: Option<f64>,
}

#[derive(Deserialize, Debug)]
pub struct Audio {
    codec: String,
    profile: Option<String>,
    channels: Option<u32>,
    sample_rate: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Nginx {
    /// Url to the NGINX stats page
    pub stats_url: String,

    /// Stream application
    pub application: String,

    /// Stream key
    pub key: String,
}

impl Nginx {
    /// 0 bitrate means the stream just started.
    /// the stats update every 10 seconds.
    pub async fn get_stats(&self) -> Option<NginxRtmpStream> {
        //TODO: keep the reqwest object around for future requests
        let res = match reqwest::get(&self.stats_url).await {
            Ok(res) => res,
            Err(_) => {
                error!("Stats page ({}) is unreachable", self.stats_url);
                return None;
            }
        };

        if res.status() != reqwest::StatusCode::OK {
            error!("Error accessing stats page ({})", self.stats_url);
            return None;
        }

        let text = res.text().await.ok()?;
        let parsed: NginxRtmpStats = match quick_xml::de::from_str(&text) {
            Ok(stats) => stats,
            Err(error) => {
                trace!("{}", &text);
                error!("Error parsing stats ({}) {}", self.stats_url, error);
                return None;
            }
        };

        let filter: Option<NginxRtmpStream> = parsed
            .server
            .application
            .into_iter()
            .filter_map(|x| {
                if x.name == self.application {
                    x.live.stream
                } else {
                    None
                }
            })
            .flatten()
            .filter(|x| x.name == self.key)
            .collect::<Vec<NginxRtmpStream>>()
            .pop();

        trace!("{:#?}", filter);
        filter
    }
}

#[async_trait]
#[typetag::serde]
impl SwitchLogic for Nginx {
    /// Which scene to switch to
    async fn switch(&self, triggers: &Triggers) -> SwitchType {
        let stats = match self.get_stats().await {
            Some(b) => b,
            None => return SwitchType::Offline,
        };

        let bitrate = stats.bw_video / 1024;

        if let Some(offline) = triggers.offline {
            if bitrate > 0 && bitrate <= offline {
                return SwitchType::Offline;
            }
        }

        if bitrate == 0 {
            return SwitchType::Previous;
        }

        if let Some(low) = triggers.low {
            if bitrate <= low {
                return SwitchType::Low;
            }
        }

        return SwitchType::Normal;
    }
}

#[async_trait]
#[typetag::serde]
impl StreamServersCommands for Nginx {
    async fn bitrate(&self) -> super::Bitrate {
        let stats = match self.get_stats().await {
            Some(stats) => stats,
            None => return super::Bitrate { message: None },
        };

        let bitrate = stats.bw_video / 1024;
        super::Bitrate {
            message: Some(format!("{}", bitrate)),
        }
    }

    async fn source_info(&self) -> String {
        todo!()
    }
}

#[typetag::serde]
impl Bsl for Nginx {}

// impl From<db::StreamServer> for Nginx {
//     fn from(item: db::StreamServer) -> Self {
//         Self {
//             stats_url: item.stats_url,
//             application: item.application,
//             key: item.key,
//             name: item.name,
//             priority: item.priority,
//         }
//     }
// }