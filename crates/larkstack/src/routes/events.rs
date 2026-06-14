//! Server-Sent Events stream of the console event log.

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, header},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures_util::stream::{Stream, StreamExt};
use serde::Deserialize;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use utoipa::IntoParams;

use crate::HostState;

/// How many historical events to replay before streaming live ones.
const BACKFILL_LIMIT: usize = 200;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct EventsQuery {
    /// Replay events with id greater than this (same role as `Last-Event-ID`).
    since: Option<u64>,
}

/// `GET /api/events` — backfill recent events, then stream live ones. Honors
/// `?since=<id>` and the `Last-Event-ID` header for reconnect backfill.
///
/// Each message's `data` is a JSON `Event`
/// `{ id, level, subsystem, target, message, fields, timestamp }`. A `lagged`
/// event signals the client fell behind and some events were dropped.
#[utoipa::path(
    get, path = "/events", tag = "console",
    security(("session" = [])),
    params(EventsQuery),
    responses((
        status = 200,
        description = "SSE stream of console events",
        content_type = "text/event-stream",
        body = Object,
    )),
)]
pub(crate) async fn events(
    State(s): State<HostState>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let live_rx = s.control.subscribe();

    let since = q.since.or_else(|| {
        headers
            .get(header::HeaderName::from_static("last-event-id"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
    });

    let backfill = match since {
        Some(id) => s.store.since(id, BACKFILL_LIMIT).await.unwrap_or_default(),
        None => s.store.recent(BACKFILL_LIMIT).await.unwrap_or_default(),
    };
    let last_replayed = backfill.last().map(|e| e.id).unwrap_or(0);

    let history = futures_util::stream::iter(backfill.into_iter().map(|ev| {
        Ok::<_, Infallible>(
            SseEvent::default()
                .id(ev.id.to_string())
                .json_data(&ev)
                .unwrap_or_else(|_| SseEvent::default()),
        )
    }));

    let live = BroadcastStream::new(live_rx).filter_map(move |res| {
        let item = match res {
            Ok(ev) => {
                if ev.id <= last_replayed {
                    None
                } else {
                    Some(Ok(SseEvent::default()
                        .id(ev.id.to_string())
                        .json_data(&ev)
                        .unwrap_or_else(|_| SseEvent::default())))
                }
            }
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                Some(Ok(SseEvent::default().event("lagged").data(n.to_string())))
            }
        };
        async move { item }
    });

    Sse::new(history.chain(live)).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
