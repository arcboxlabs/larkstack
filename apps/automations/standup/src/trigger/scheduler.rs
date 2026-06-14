//! 4 daily triggers in Asia/Shanghai:
//! - 20:00 create next-day doc + announce
//! - 22:00 remind unfilled (next-day doc)
//! - 09:30 remind unfilled (today's doc)
//! - 10:00 final reminder + in-app urgent escalation

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use chrono_tz::Asia::Shanghai;
use larkoapi::LarkBotClient;
use tracing::{error, info, warn};

use crate::config::StandupConfig;
use crate::flow;

#[derive(Clone, Copy)]
enum TargetDay {
    Today,
    Tomorrow,
}

#[derive(Clone, Copy)]
enum Action {
    Announce,
    Remind,
    UrgentRemind,
}

pub async fn run(config: StandupConfig, client: Arc<LarkBotClient>) {
    if !config.enabled {
        info!("standup scheduler disabled (STANDUP_ENABLED=false)");
        return;
    }
    if config.chat_id.is_none() || config.folder_token.is_none() {
        warn!("standup: STANDUP_CHAT_ID or STANDUP_FOLDER_TOKEN missing, scheduler will not run");
        return;
    }
    info!(
        "standup scheduler starting (chat={:?}, folder={:?})",
        config.chat_id, config.folder_token
    );

    let cfg = Arc::new(config);
    let triggers: [(&'static str, u32, u32, TargetDay, Action); 4] = [
        ("announce-20", 20, 0, TargetDay::Tomorrow, Action::Announce),
        ("remind-22", 22, 0, TargetDay::Tomorrow, Action::Remind),
        ("remind-0930", 9, 30, TargetDay::Today, Action::Remind),
        ("urgent-10", 10, 0, TargetDay::Today, Action::UrgentRemind),
    ];

    let mut handles = Vec::new();
    for (name, hour, minute, target, action) in triggers {
        let client = Arc::clone(&client);
        let cfg = Arc::clone(&cfg);
        handles.push(tokio::spawn(async move {
            run_trigger(name, hour, minute, target, action, cfg, client).await;
        }));
    }

    // Keep main alive until all triggers exit (they don't under normal operation).
    for h in handles {
        let _ = h.await;
    }
}

async fn run_trigger(
    name: &'static str,
    hour: u32,
    minute: u32,
    target: TargetDay,
    action: Action,
    cfg: Arc<StandupConfig>,
    client: Arc<LarkBotClient>,
) {
    loop {
        let wait = duration_until(hour, minute);
        info!("standup[{name}] next fire in {:?}", wait);
        tokio::time::sleep(wait).await;

        let now_sh = Utc::now().with_timezone(&Shanghai);
        let today = now_sh.date_naive();
        let target_date = match target {
            TargetDay::Today => today,
            TargetDay::Tomorrow => today + ChronoDuration::days(1),
        };

        let result = match action {
            Action::Announce => flow::announce(&cfg, &client, target_date).await,
            Action::Remind => flow::remind(&cfg, &client, target_date, false).await,
            Action::UrgentRemind => flow::remind(&cfg, &client, target_date, true).await,
        };
        if let Err(e) = result {
            error!("standup[{name}] {target_date}: {e}");
        }
        // Sleep a few seconds past the trigger so the next wait-calc lands on
        // tomorrow, not re-firing the same trigger.
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn duration_until(hour: u32, minute: u32) -> Duration {
    let now = Utc::now().with_timezone(&Shanghai);
    let today_ndt = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .expect("valid h:m");
    let target = Shanghai
        .from_local_datetime(&today_ndt)
        .single()
        .filter(|t| *t > now)
        .unwrap_or_else(|| {
            let ndt = (now.date_naive() + ChronoDuration::days(1))
                .and_hms_opt(hour, minute, 0)
                .expect("valid h:m");
            Shanghai
                .from_local_datetime(&ndt)
                .single()
                .expect("Asia/Shanghai has no DST")
        });
    (target - now).to_std().unwrap_or(Duration::from_secs(60))
}
