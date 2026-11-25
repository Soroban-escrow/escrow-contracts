use std::{format, fs, println, string::String, vec, vec::Vec};

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

use super::setup::*;
use crate::{BountyEscrow, BountyEscrowClient};

/// The committed snapshot. It is deliberately a plain file so a reviewer can
/// see every number in the diff; the parser below only understands this shape.
const BUDGETS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/budgets.json");
const TOLERANCE_PERCENT: u64 = 10;
const WASM: &[u8] = include_bytes!("../../../../target/wasm32v1-none/release/bounty_escrow.wasm");

struct Measurement {
    name: &'static str,
    cpu_instructions: u64,
    memory_bytes: u64,
    read_entries: u64,
    write_entries: u64,
    read_bytes: u64,
    write_bytes: u64,
}

struct SnapshotEntry {
    name: String,
    cpu_instructions: u64,
    memory_bytes: u64,
    read_entries: u64,
    write_entries: u64,
    read_bytes: u64,
    write_bytes: u64,
}

/// Reads the resources metered for the last top-level invocation. The budget
/// resets before every invocation, so setup calls do not leak into the numbers.
fn record(name: &'static str, c: &Ctx<'_>) -> Measurement {
    let resources = c.env.cost_estimate().resources();
    let budget = c.env.cost_estimate().budget();
    Measurement {
        name,
        cpu_instructions: budget.cpu_instruction_cost(),
        memory_bytes: budget.memory_bytes_cost(),
        read_entries: u64::from(resources.disk_read_entries)
            + u64::from(resources.memory_read_entries),
        write_entries: u64::from(resources.write_entries),
        read_bytes: u64::from(resources.disk_read_bytes),
        write_bytes: u64::from(resources.write_bytes),
    }
}

fn measure_init() -> Measurement {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let id = env.register(BountyEscrow, ());
    let client = BountyEscrowClient::new(&env, &id);
    let c = Ctx {
        admin: Address::generate(&env),
        relayer: Address::generate(&env),
        arbiter: Address::generate(&env),
        treasury: Address::generate(&env),
        maintainer: Address::generate(&env),
        winner: Address::generate(&env),
        token: Address::generate(&env),
        token2: Address::generate(&env),
        env,
        client,
    };
    c.client.init(&crate::InitParams {
        admin: c.admin.clone(),
        relayer: c.relayer.clone(),
        arbiter: c.arbiter.clone(),
        treasury: c.treasury.clone(),
        fee_bps: FEE_BPS,
        grace_secs: GRACE,
        min_duration_secs: DAY,
        max_duration_secs: 90 * DAY,
    });
    record("init", &c)
}

fn measure_fund() -> Measurement {
    let c = Ctx::new();
    c.fund(1);
    record("fund_bounty", &c)
}

fn measure_release(with_fee: bool) -> Measurement {
    let c = Ctx::new();
    if !with_fee {
        c.client.set_fee(&c.admin, &0);
    }
    let id = c.fund(1);
    c.client.release(&c.maintainer, &id, &c.winner);
    record(
        if with_fee {
            "release_with_fee"
        } else {
            "release_without_fee"
        },
        &c,
    )
}

fn measure_refund() -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.set_time(c.now() + 7 * DAY + GRACE);
    c.client.refund(&c.maintainer, &id);
    record("refund", &c)
}

fn measure_open_dispute() -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.arbiter, &id);
    record("open_dispute", &c)
}

fn measure_resolve(name: &'static str, winner_bps: u32, with_evidence: bool) -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.arbiter, &id);
    if with_evidence {
        let hash = BytesN::from_array(&c.env, &[7u8; 32]);
        c.client.resolve_with_evidence(
            &c.arbiter,
            &id,
            &Some(c.winner.clone()),
            &winner_bps,
            &hash,
        );
    } else if winner_bps == 0 {
        c.client.resolve(&c.arbiter, &id, &None, &0);
    } else {
        c.client
            .resolve(&c.arbiter, &id, &Some(c.winner.clone()), &winner_bps);
    }
    record(name, &c)
}

fn measure_top_up() -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.top_up(&c.maintainer, &id, &UNIT);
    record("top_up", &c)
}

fn measure_extend_deadline() -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client
        .extend_deadline(&c.maintainer, &id, &(c.now() + 8 * DAY));
    record("extend_deadline", &c)
}

fn measure_expire_dispute() -> Measurement {
    let c = Ctx::new();
    let id = c.fund(1);
    c.client.open_dispute(&c.arbiter, &id);
    let timeout = c.client.get_dispute_timeout();
    c.set_time(c.now() + timeout);
    c.client.expire_dispute(&c.maintainer, &id);
    record("expire_dispute", &c)
}

fn measure_set_token() -> Measurement {
    let c = Ctx::new();
    c.client
        .set_token(&c.admin, &c.token2, &UNIT, &(500 * UNIT), &None);
    record("set_token", &c)
}

fn measure_remove_token() -> Measurement {
    let c = Ctx::new();
    c.client.remove_token(&c.admin, &c.token2);
    record("remove_token", &c)
}

fn measure_set_fee() -> Measurement {
    let c = Ctx::new();
    c.client.set_fee(&c.admin, &500);
    record("set_fee", &c)
}

fn measure_set_relayer() -> Measurement {
    let c = Ctx::new();
    let new_relayer = Address::generate(&c.env);
    c.client.set_relayer(&c.admin, &new_relayer);
    record("set_relayer", &c)
}

fn measure_set_arbiter() -> Measurement {
    let c = Ctx::new();
    let new_arbiter = Address::generate(&c.env);
    c.client.set_arbiter(&c.admin, &new_arbiter);
    record("set_arbiter", &c)
}

fn measure_set_treasury() -> Measurement {
    let c = Ctx::new();
    let new_treasury = Address::generate(&c.env);
    c.client.set_treasury(&c.admin, &new_treasury);
    record("set_treasury", &c)
}

fn measure_set_grace() -> Measurement {
    let c = Ctx::new();
    c.client.set_grace(&c.admin, &(4 * DAY));
    record("set_grace", &c)
}

fn measure_set_durations() -> Measurement {
    let c = Ctx::new();
    c.client.set_durations(&c.admin, &DAY, &(90 * DAY));
    record("set_durations", &c)
}

fn measure_set_admin() -> Measurement {
    let c = Ctx::new();
    let new_admin = Address::generate(&c.env);
    c.client.set_admin(&c.admin, &new_admin);
    record("set_admin", &c)
}

fn measure_pause() -> Measurement {
    let c = Ctx::new();
    c.client.pause(&c.admin);
    record("pause", &c)
}

fn measure_unpause() -> Measurement {
    let c = Ctx::new();
    c.client.pause(&c.admin);
    c.client.unpause(&c.admin);
    record("unpause", &c)
}

fn measure_set_dispute_timeout() -> Measurement {
    let c = Ctx::new();
    c.client.set_dispute_timeout(&c.admin, &(14 * DAY));
    record("set_dispute_timeout", &c)
}

fn measure_upgrade() -> Measurement {
    let c = Ctx::new();
    let hash = c.env.deployer().upload_contract_wasm(WASM);
    c.client.upgrade(&c.admin, &hash);
    record("upgrade", &c)
}

fn measurements() -> Vec<Measurement> {
    vec![
        measure_init(),
        measure_fund(),
        measure_release(true),
        measure_release(false),
        measure_refund(),
        measure_open_dispute(),
        measure_resolve("resolve_split", 7_000, false),
        measure_resolve("resolve_zero", 0, false),
        measure_resolve("resolve_with_evidence", 7_000, true),
        measure_top_up(),
        measure_extend_deadline(),
        measure_expire_dispute(),
        measure_set_token(),
        measure_remove_token(),
        measure_set_fee(),
        measure_set_relayer(),
        measure_set_arbiter(),
        measure_set_treasury(),
        measure_set_grace(),
        measure_set_durations(),
        measure_set_admin(),
        measure_pause(),
        measure_unpause(),
        measure_set_dispute_timeout(),
        measure_upgrade(),
    ]
}

fn print_table(measurements: &[Measurement]) {
    println!(
        "{:<28} {:>14} {:>14} {:>8} {:>8} {:>12} {:>12}",
        "entry point", "cpu insns", "mem bytes", "reads", "writes", "read bytes", "write bytes"
    );
    for m in measurements {
        println!(
            "{:<28} {:>14} {:>14} {:>8} {:>8} {:>12} {:>12}",
            m.name,
            m.cpu_instructions,
            m.memory_bytes,
            m.read_entries,
            m.write_entries,
            m.read_bytes,
            m.write_bytes
        );
    }
}

fn json_string(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = &line[start..];
    let first = rest.find('"')?;
    let rest = &rest[first + 1..];
    let second = rest.find('"')?;
    Some(String::from(&rest[..second]))
}

fn json_u64(line: &str, key: &str) -> Option<u64> {
    let start = line.find(key)? + key.len();
    let digits: String = line[start..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn parse_snapshot(text: &str) -> (u64, Vec<SnapshotEntry>) {
    let tolerance = text
        .lines()
        .find(|line| line.contains("\"tolerance_percent\""))
        .and_then(|line| json_u64(line, "\"tolerance_percent\""))
        .unwrap_or(TOLERANCE_PERCENT);
    let entries = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("{\"name\""))
        .map(|line| SnapshotEntry {
            name: json_string(line, "\"name\"").unwrap_or_default(),
            cpu_instructions: json_u64(line, "\"cpu_instructions\"").unwrap_or_default(),
            memory_bytes: json_u64(line, "\"memory_bytes\"").unwrap_or_default(),
            read_entries: json_u64(line, "\"read_entries\"").unwrap_or_default(),
            write_entries: json_u64(line, "\"write_entries\"").unwrap_or_default(),
            read_bytes: json_u64(line, "\"read_bytes\"").unwrap_or_default(),
            write_bytes: json_u64(line, "\"write_bytes\"").unwrap_or_default(),
        })
        .collect();
    (tolerance, entries)
}

fn render_snapshot(measurements: &[Measurement], tolerance: u64) -> String {
    let mut out = String::from(
        "{\n  \"version\": 1,\n  \"note\": \"Budgets measured by contracts/bounty_escrow/src/test/budgets.rs. Refresh deliberately with UPDATE_BUDGETS=1 cargo test --package bounty_escrow budgets -- --nocapture.\",\n",
    );
    out.push_str(&format!("  \"tolerance_percent\": {tolerance},\n"));
    out.push_str("  \"entries\": [\n");
    for (index, m) in measurements.iter().enumerate() {
        out.push_str(&format!(
            "    {{\"name\": \"{}\", \"cpu_instructions\": {}, \"memory_bytes\": {}, \"read_entries\": {}, \"write_entries\": {}, \"read_bytes\": {}, \"write_bytes\": {}}}",
            m.name,
            m.cpu_instructions,
            m.memory_bytes,
            m.read_entries,
            m.write_entries,
            m.read_bytes,
            m.write_bytes
        ));
        if index + 1 < measurements.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

fn check(
    failures: &mut Vec<String>,
    name: &str,
    metric: &str,
    actual: u64,
    baseline: u64,
    tolerance: u64,
) {
    let allowed = baseline.saturating_mul(100 + tolerance) / 100;
    if actual > allowed {
        failures.push(format!(
            "{name}: {metric} {actual} exceeds {baseline} by more than {tolerance}% (allowed {allowed})"
        ));
    }
}

#[test]
fn resource_budgets_match_snapshot() {
    let measurements = measurements();
    print_table(&measurements);

    if std::env::var("UPDATE_BUDGETS").ok().as_deref() == Some("1") {
        fs::write(
            BUDGETS_PATH,
            render_snapshot(&measurements, TOLERANCE_PERCENT),
        )
        .unwrap();
        println!("wrote {BUDGETS_PATH}");
        return;
    }

    let text = fs::read_to_string(BUDGETS_PATH).unwrap_or_else(|error| {
        panic!(
            "cannot read {BUDGETS_PATH}: {error}. Run UPDATE_BUDGETS=1 cargo test --package bounty_escrow budgets -- --nocapture to create it."
        )
    });
    let (tolerance, snapshot) = parse_snapshot(&text);
    let mut failures = Vec::new();
    for m in &measurements {
        let Some(baseline) = snapshot.iter().find(|entry| entry.name.as_str() == m.name) else {
            failures.push(format!("{}: no baseline in {BUDGETS_PATH}", m.name));
            continue;
        };
        check(
            &mut failures,
            m.name,
            "cpu_instructions",
            m.cpu_instructions,
            baseline.cpu_instructions,
            tolerance,
        );
        check(
            &mut failures,
            m.name,
            "memory_bytes",
            m.memory_bytes,
            baseline.memory_bytes,
            tolerance,
        );
        check(
            &mut failures,
            m.name,
            "read_entries",
            m.read_entries,
            baseline.read_entries,
            tolerance,
        );
        check(
            &mut failures,
            m.name,
            "write_entries",
            m.write_entries,
            baseline.write_entries,
            tolerance,
        );
        check(
            &mut failures,
            m.name,
            "read_bytes",
            m.read_bytes,
            baseline.read_bytes,
            tolerance,
        );
        check(
            &mut failures,
            m.name,
            "write_bytes",
            m.write_bytes,
            baseline.write_bytes,
            tolerance,
        );
    }
    if !failures.is_empty() {
        panic!("resource budget regression:\n{}", failures.join("\n"));
    }
}
