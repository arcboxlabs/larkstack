//! The daily triggers. Each of the four jobs ([`Job`]) sleeps until its
//! configured wall-clock time and fires the matching [`flow`] operation. Times,
//! per-job enable flags, and the timezone come from the live
//! [`settings`](crate::settings) blob, reloaded every pass so console edits take
//! effect without a restart.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use larkoapi::LarkBotClient;
use larkstack_core::StateStore;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::config::StandupConfig;
use crate::flow;
use crate::settings::{self, Job};

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

/// The four jobs and their intrinsic `(target day, action)`; the time + enabled
/// flag are read per-pass from [`settings`].
const JOBS: [(&str, Job, TargetDay, Action); 4] = [
    (
        "announce",
        Job::Announce,
        TargetDay::Tomorrow,
        Action::Announce,
    ),
    (
        "remind-evening",
        Job::RemindEvening,
        TargetDay::Tomorrow,
        Action::Remind,
    ),
    (
        "remind-morning",
        Job::RemindMorning,
        TargetDay::Today,
        Action::Remind,
    ),
    (
        "urgent",
        Job::Urgent,
        TargetDay::Today,
        Action::UrgentRemind,
    ),
];

/// How long a disabled job waits before re-checking whether it was re-enabled.
const DISABLED_POLL: Duration = Duration::from_secs(60);

pub async fn run(
    config: StandupConfig,
    store: Arc<dyn StateStore>,
    client: Arc<LarkBotClient>,
    cancel: CancellationToken,
) {
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
    let mut handles = Vec::new();
    for (name, job, target, action) in JOBS {
        let client = Arc::clone(&client);
        let cfg = Arc::clone(&cfg);
        let store = Arc::clone(&store);
        let cancel = cancel.clone();
        handles.push(tokio::spawn(async move {
            run_trigger(name, job, target, action, cfg, store, client, cancel).await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_trigger(
    name: &'static str,
    job: Job,
    target: TargetDay,
    action: Action,
    cfg: Arc<StandupConfig>,
    store: Arc<dyn StateStore>,
    client: Arc<LarkBotClient>,
    cancel: CancellationToken,
) {
    loop {
        let s = settings::load(&store).await;
        let trig = s.trigger(job);
        if !trig.enabled {
            tokio::select! {
                _ = tokio::time::sleep(DISABLED_POLL) => continue,
                _ = cancel.cancelled() => return,
            }
        }

        let wait = duration_until(trig.hour, trig.minute, s.timezone);
        info!("standup[{name}] next fire in {wait:?}");
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = cancel.cancelled() => return,
        }

        // Reload after waking so edits made during the wait (time, templates,
        // a toggle-off) are honored.
        let s = settings::load(&store).await;
        if !s.trigger(job).enabled {
            continue;
        }
        let today = Utc::now().with_timezone(&s.timezone).date_naive();
        let target_date = match target {
            TargetDay::Today => today,
            TargetDay::Tomorrow => today + ChronoDuration::days(1),
        };
        let result = match action {
            Action::Announce => flow::announce(&cfg, &s, &client, target_date).await,
            Action::Remind => flow::remind(&cfg, &s, &client, target_date, false).await,
            Action::UrgentRemind => flow::remind(&cfg, &s, &client, target_date, true).await,
        };
        if let Err(e) = result {
            error!("standup[{name}] {target_date}: {e}");
        }

        // Sleep a few seconds past the trigger so the next wait-calc lands on
        // tomorrow, not re-firing the same trigger.
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = cancel.cancelled() => return,
        }
    }
}

/// Time until the next `hour:minute` in `tz` (today if still ahead, else tomorrow).
fn duration_until(hour: u32, minute: u32, tz: Tz) -> Duration {
    let now = Utc::now().with_timezone(&tz);
    let mut target = local_instant(tz, now.date_naive(), hour, minute);
    if target <= now {
        target = local_instant(tz, now.date_naive() + ChronoDuration::days(1), hour, minute);
    }
    (target - now).to_std().unwrap_or(Duration::from_secs(60))
}

/// Resolve `date` + `hour:minute` to an instant in `tz`, tolerating DST edges:
/// ambiguous times pick the earlier instant; a nonexistent time (spring-forward
/// gap) fires an hour later.
fn local_instant(tz: Tz, date: NaiveDate, hour: u32, minute: u32) -> DateTime<Tz> {
    let ndt = date.and_hms_opt(hour, minute, 0).unwrap_or_default();
    tz.from_local_datetime(&ndt).earliest().unwrap_or_else(|| {
        let bumped = ndt + ChronoDuration::hours(1);
        tz.from_local_datetime(&bumped)
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&bumped))
    })
}
