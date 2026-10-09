//! AI usage (the Usage page, GA-33): the tokens and API cost of the agents' runs and chat turns, in total, per day, per
//! agent and per project, summed in SQL over `runs`. Nothing here changes a budget: a run's cost still rolls up into its
//! agent's budget only (`runs::agent_spend_since`); the project totals are only shown.
//!
//! - The cost is the CLI's own estimate at API prices (Claude Code's `total_cost_usd`), not a bill: on a subscription it is
//!   not what gets paid.
//! - Input tokens include cache reads and writes: the stream parsers add them up, so `cache_read_tokens` and
//!   `cache_write_tokens` stay empty.
//! - A run with tokens but no cost has an unknown cost, not $0: Codex, Gemini and Other CLIs report no cost (their runs
//!   save 0), while Claude Code always reports one, and tokens always cost something.
//! - Days and months are UTC, like the agents' monthly budgets (`runs::month_start_ms`).
use std::ops::AddAssign;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

const DAY_MS: i64 = 86_400_000;

/// The most days `Usage::days` holds (a period of 30 days needs 30, a month 31).
pub const MAX_DAYS: i64 = 400;

/// The periods of the Usage page's switch: today, the last 7 or 30 days (today included), this month.
pub const PERIODS: [&str; 4] = ["today", "7d", "30d", "month"];

/// SQL that is true for a run of `runs` (aliased `alias`) whose cost is unknown: tokens but no cost.
pub(crate) fn unknown_cost(alias: &str) -> String {
    format!("(COALESCE({alias}.cost_usd_micros, 0) = 0 AND COALESCE({alias}.input_tokens, 0) + COALESCE({alias}.output_tokens, 0) > 0)")
}

/// What a set of runs used. A chat turn of the Team Lead is a run too (`chat_turns` counts them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    /// Runs, chat turns included.
    pub runs: i64,
    /// Of `runs`, the chat turns.
    pub chat_turns: i64,
    /// Input tokens, cache reads and writes included.
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// The API cost of the runs that reported one: what their tokens would cost at API prices, not a bill.
    pub cost_usd_micros: i64,
    /// Runs with tokens but no cost (a CLI that reports none): their cost is unknown, and `cost_usd_micros` leaves it out.
    pub unknown_cost_runs: i64,
}

impl AddAssign for UsageTotals {
    fn add_assign(&mut self, o: UsageTotals) {
        self.runs += o.runs;
        self.chat_turns += o.chat_turns;
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cost_usd_micros += o.cost_usd_micros;
        self.unknown_cost_runs += o.unknown_cost_runs;
    }
}

impl std::iter::Sum for UsageTotals {
    fn sum<I: Iterator<Item = UsageTotals>>(iter: I) -> UsageTotals {
        iter.fold(UsageTotals::default(), |mut a, b| { a += b; a })
    }
}

/// One UTC day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    /// Midnight UTC (Unix ms).
    pub day_start: i64,
    pub totals: UsageTotals,
}

/// One agent, its chat turns and board checks included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsage {
    pub agent_id: String,
    pub name: String,
    /// Its role on the team (lead, frontend, …); for an agent on no team, the role its runs acted in.
    pub role_key: Option<String>,
    pub totals: UsageTotals,
}

/// One project: the runs on its cards (archived cards included).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUsage {
    pub project_id: String,
    pub number: String,
    pub key: String,
    pub name: String,
    pub color: Option<String>,
    pub totals: UsageTotals,
}

/// The Usage page for one period. The agents add up to `total`, and so do the projects with `chat` and `no_project`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    /// The runs created from `since` up to (not including) `until`, Unix ms.
    pub since: i64,
    pub until: i64,
    /// Everything: the runs and chat turns of all agents.
    pub total: UsageTotals,
    /// Per day from `since` (24 hours each, so UTC days when `since` is a UTC midnight), oldest first; days without runs
    /// are zero. At most `MAX_DAYS`.
    pub days: Vec<UsageDay>,
    /// Per agent with runs, the highest cost first.
    pub agents: Vec<AgentUsage>,
    /// Per project with runs, the highest cost first.
    pub projects: Vec<ProjectUsage>,
    /// Runs without a card, so without a project: the Team Lead's chat turns and board checks.
    pub chat: UsageTotals,
    /// Runs on cards without a project.
    pub no_project: UsageTotals,
}

/// The start and end (exclusive) of a period of the switch at `now`, in whole UTC days up to the end of today: `today`;
/// `7d` and `30d`, today and the 6 or 29 days before it; `month`, from the 1st of this month.
pub fn period_range(period: &str, now: i64) -> Result<(i64, i64)> {
    let today = now - now.rem_euclid(DAY_MS);
    let since = match period {
        "today" => today,
        "7d" => today - 6 * DAY_MS,
        "30d" => today - 29 * DAY_MS,
        "month" => crate::runs::month_start_ms(now),
        _ => return Err(Error::Invalid(format!("unknown period {period}: use today, 7d, 30d or month"))),
    };
    Ok((since, today + DAY_MS))
}

/// The Usage page for a period of the switch (`PERIODS`) at `now`.
pub fn for_period(db: &Db, period: &str, now: i64) -> Result<Usage> {
    let (since, until) = period_range(period, now)?;
    summary(db, since, until)
}

/// What the runs created from `since` up to (not including) `until` used: in total, per day, per agent and per project.
pub fn summary(db: &Db, since: i64, until: i64) -> Result<Usage> {
    if until <= since {
        return Err(Error::Invalid("the period ends before it starts".into()));
    }
    let unknown = unknown_cost("r");
    let sums = format!("count(*), COALESCE(SUM(r.trigger = 'chat'), 0), COALESCE(SUM(r.input_tokens), 0), COALESCE(SUM(r.output_tokens), 0),
                        COALESCE(SUM(r.cost_usd_micros), 0), COALESCE(SUM({unknown}), 0)");
    let period = "r.created_at >= ?1 AND r.created_at < ?2";
    let totals = |r: &rusqlite::Row, at: usize| -> rusqlite::Result<UsageTotals> {
        Ok(UsageTotals { runs: r.get(at)?, chat_turns: r.get(at + 1)?, input_tokens: r.get(at + 2)?, output_tokens: r.get(at + 3)?,
                         cost_usd_micros: r.get(at + 4)?, unknown_cost_runs: r.get(at + 5)? })
    };
    db.read(|c| {
        let total = c.query_row(&format!("SELECT {sums} FROM runs r WHERE {period}"), params![since, until], |r| totals(r, 0))?;

        let n_days = ((until as i128 - since as i128 + DAY_MS as i128 - 1) / DAY_MS as i128).min(MAX_DAYS as i128) as i64;
        let mut days: Vec<UsageDay> = (0..n_days).map(|i| UsageDay { day_start: since + i * DAY_MS, totals: UsageTotals::default() }).collect();
        let mut st = c.prepare(&format!("SELECT (r.created_at - ?1) / {DAY_MS}, {sums} FROM runs r WHERE {period} GROUP BY 1"))?;
        for row in st.query_map(params![since, until], |r| Ok((r.get::<_, i64>(0)?, totals(r, 1)?)))? {
            let (day, t) = row?;
            if let Some(d) = usize::try_from(day).ok().and_then(|i| days.get_mut(i)) {
                d.totals = t;
            }
        }

        let mut st = c.prepare(&format!(
            "SELECT r.agent_actor_id, COALESCE(a.name, 'A removed agent'),
                    COALESCE((SELECT m.role_key FROM team_members m WHERE m.actor_id = r.agent_actor_id ORDER BY m.is_lead DESC LIMIT 1), MAX(r.role_key)),
                    {sums}
             FROM runs r LEFT JOIN actors a ON a.id = r.agent_actor_id
             WHERE {period} GROUP BY r.agent_actor_id"))?;
        let mut agents = st.query_map(params![since, until], |r| Ok(AgentUsage {
            agent_id: r.get(0)?, name: r.get(1)?, role_key: r.get(2)?, totals: totals(r, 3)?,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        agents.sort_by(|a, b| most_first(&a.totals, &b.totals).then_with(|| a.name.cmp(&b.name)));

        // Grouped by card or none, then by project: runs without a card are the chat line, runs on a card without a
        // project the no-project line.
        let mut st = c.prepare(&format!(
            "SELECT r.task_id IS NULL, t.project_id, COALESCE(p.number, ''), COALESCE(p.key, ''), COALESCE(p.name, 'A removed project'), p.color,
                    {sums}
             FROM runs r LEFT JOIN tasks t ON t.id = r.task_id LEFT JOIN projects p ON p.id = t.project_id
             WHERE {period} GROUP BY r.task_id IS NULL, t.project_id"))?;
        let (mut projects, mut chat, mut no_project) = (vec![], UsageTotals::default(), UsageTotals::default());
        let rows = st.query_map(params![since, until], |r| Ok((r.get::<_, bool>(0)?, r.get::<_, Option<String>>(1)?, ProjectUsage {
            project_id: String::new(), number: r.get(2)?, key: r.get(3)?, name: r.get(4)?, color: r.get(5)?, totals: totals(r, 6)?,
        })))?;
        for row in rows {
            match row? {
                (true, _, p) => chat += p.totals,
                (false, None, p) => no_project += p.totals,
                (false, Some(id), p) => projects.push(ProjectUsage { project_id: id, ..p }),
            }
        }
        projects.sort_by(|a, b| most_first(&a.totals, &b.totals).then_with(|| a.name.cmp(&b.name)));

        Ok(Usage { since, until, total, days, agents, projects, chat, no_project })
    })
}

/// The highest cost first, then the most tokens.
fn most_first(a: &UsageTotals, b: &UsageTotals) -> std::cmp::Ordering {
    b.cost_usd_micros.cmp(&a.cost_usd_micros).then_with(|| (b.input_tokens + b.output_tokens).cmp(&(a.input_tokens + a.output_tokens)))
}
