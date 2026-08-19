// SPDX-License-Identifier: Apache-2.0

//! ggen Marketplace-backed cloud integration for Weaver.
//!
//! The lifecycle is intentionally split into SELECT, CONSTRUCT, and DO:
//! - `plan` admits an exact marketplace snapshot and an explicit set of packs.
//! - `construct` replays that admission and runs `ggen sync run`; generated
//!   artifacts have no ambient cloud execution authority.
//! - `actuate` can cross the DO boundary only through a BRCE runner. Weaver
//!   never invokes Terraform, kubectl, or a cloud-provider CLI directly.

use crate::cli::{CloudActuateCommand, CloudCommand, CloudConstructCommand, CloudPlanCommand};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PLAN_SCHEMA: &str = "weaver.cloud.plan.v1";
const CONSTRUCTION_SCHEMA: &str = "weaver.cloud.construction-receipt.v1";
const BRCE_INTENT_SCHEMA: &str = "weaver.brce.cloud-intent.v1";
const BRCE_RECEIPT_SCHEMA: &str = "weaver.brce.cloud-receipt.v1";
const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PackIdentity {
    name: String,
    version: String,
    profile: String,
    path: String,
    digest: String,
    manifest_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MarketplaceIdentity {
    root: String,
    catalog_schema: String,
    version: String,
    fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudPlan {
    schema: String,
    provider: String,
    marketplace: MarketplaceIdentity,
    packs: Vec<PackIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CloudSubject {
    provider: String,
    marketplace_fingerprint: String,
    marketplace_version: String,
    packs: Vec<PackIdentity>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConstructionReceipt {
    schema: String,
    phase: String,
    standing: String,
    subject: CloudSubject,
    plan_path: String,
    marketplace_root: String,
    workspace: String,
    ggen_config: String,
    ggen_command: Vec<String>,
    ggen_exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BrceIntent {
    schema: String,
    authority_boundary: String,
    operation: String,
    subject: CloudSubject,
    construction_receipt: String,
    workspace: String,
    ggen_config: String,
}

#[derive(Debug, Deserialize)]
struct BrceReceipt {
    schema: String,
    receipt_id: String,
    actuator: String,
    standing: String,
    operation: String,
    subject: CloudSubject,
    authority: Value,
    consequence: Value,
    replay: Value,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    schema: String,
    marketplace_version: String,
    packs: Vec<CatalogPack>,
}

#[derive(Debug, Deserialize)]
struct CatalogPack {
    name: String,
    version: String,
    profile: String,
    path: String,
    digest: String,
    manifest_sha256: String,
}

struct MarketplaceSnapshot {
    catalog_schema: String,
    version: String,
    fingerprint: String,
    packs: Vec<PackIdentity>,
}

impl From<CatalogPack> for PackIdentity {
    fn from(pack: CatalogPack) -> Self {
        Self {
            name: pack.name,
            version: pack.version,
            profile: pack.profile,
            path: pack.path,
            digest: pack.digest,
            manifest_sha256: pack.manifest_sha256,
        }
    }
}

pub(crate) fn command(command: &CloudCommand) -> Result<(), String> {
    match command {
        CloudCommand::Plan(args) => plan(args),
        CloudCommand::Construct(args) => construct(args),
        CloudCommand::Actuate(args) => actuate(args),
    }
}

fn plan(args: &CloudPlanCommand) -> Result<(), String> {
    let provider = args.provider.trim();
    if provider.is_empty() {
        return Err("REFUSED:CLOUD_PROVIDER_EMPTY".to_owned());
    }

    let marketplace_root = canonical_dir(&args.marketplace, "MARKETPLACE_ROOT")?;
    let snapshot = load_marketplace(&marketplace_root, &args.python)?;

    let mut requested = BTreeSet::new();
    for raw_name in &args.packs {
        let name = raw_name.trim();
        if name.is_empty() {
            return Err("REFUSED:PACK_NAME_EMPTY".to_owned());
        }
        if !requested.insert(name.to_owned()) {
            return Err(format!("REFUSED:DUPLICATE_PACK_SELECTION:{name}"));
        }
    }
    if requested.is_empty() {
        return Err("REFUSED:NO_PACKS_SELECTED".to_owned());
    }

    let available = snapshot
        .packs
        .into_iter()
        .map(|pack| (pack.name.clone(), pack))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::with_capacity(requested.len());
    for name in requested {
        let pack = available
            .get(&name)
            .ok_or_else(|| format!("REFUSED:PACK_NOT_IN_ADMITTED_CATALOG:{name}"))?;
        selected.push(pack.clone());
    }

    let root = path_to_utf8(&marketplace_root, "MARKETPLACE_ROOT_UTF8")?;
    let plan = CloudPlan {
        schema: PLAN_SCHEMA.to_owned(),
        provider: provider.to_owned(),
        marketplace: MarketplaceIdentity {
            root,
            catalog_schema: snapshot.catalog_schema,
            version: snapshot.version,
            fingerprint: snapshot.fingerprint,
        },
        packs: selected,
    };

    write_json_new(&args.output, &plan)?;
    println!(
        "SELECT_ALIVE provider={} packs={} plan={}",
        plan.provider,
        plan.packs.len(),
        args.output.display()
    );
    Ok(())
}

fn construct(args: &CloudConstructCommand) -> Result<(), String> {
    let plan: CloudPlan = read_json(&args.plan)?;
    let marketplace_root = verify_plan(&plan, &args.python)?;

    fs::create_dir_all(&args.workspace).map_err(|error| {
        format!(
            "BLOCKED:WORKSPACE_CREATE:{}:{error}",
            args.workspace.display()
        )
    })?;
    let workspace = canonical_dir(&args.workspace, "WORKSPACE")?;
    let ggen_config = workspace.join("ggen.toml");
    let config = render_ggen_config(&plan, &marketplace_root)?;
    write_new_or_identical(&ggen_config, config.as_bytes())?;

    let receipt_path = args
        .receipt
        .clone()
        .unwrap_or_else(|| workspace.join("weaver-construction-receipt.json"));
    ensure_new_path(&receipt_path, "CONSTRUCTION_RECEIPT_PATH_EXISTS")?;

    let ggen_program = args.ggen.to_string_lossy().into_owned();
    let ggen_command = vec![ggen_program, "sync".to_owned(), "run".to_owned()];
    let execution = Command::new(&args.ggen)
        .args(["sync", "run"])
        .current_dir(&workspace)
        .output();

    let subject = subject_from_plan(&plan);
    let plan_path = path_to_display(&args.plan);
    let marketplace_root_display = path_to_utf8(&marketplace_root, "MARKETPLACE_ROOT_UTF8")?;
    let workspace_display = path_to_utf8(&workspace, "WORKSPACE_UTF8")?;
    let ggen_config_display = path_to_utf8(&ggen_config, "GGEN_CONFIG_UTF8")?;

    let (standing, exit_code, stdout, stderr, error) = match execution {
        Ok(output) => {
            let code = output.status.code();
            let standing = if output.status.success() {
                "ALIVE"
            } else {
                "BUILD_BROKEN"
            };
            (
                standing.to_owned(),
                code,
                capture(&output.stdout),
                capture(&output.stderr),
                None,
            )
        }
        Err(error) => (
            "BLOCKED".to_owned(),
            None,
            String::new(),
            String::new(),
            Some(error.to_string()),
        ),
    };

    let receipt = ConstructionReceipt {
        schema: CONSTRUCTION_SCHEMA.to_owned(),
        phase: "CONSTRUCT".to_owned(),
        standing: standing.clone(),
        subject,
        plan_path,
        marketplace_root: marketplace_root_display,
        workspace: workspace_display,
        ggen_config: ggen_config_display,
        ggen_command,
        ggen_exit_code: exit_code,
        stdout,
        stderr,
        error: error.clone(),
    };
    write_json_new(&receipt_path, &receipt)?;

    match standing.as_str() {
        "ALIVE" => {
            println!(
                "CONSTRUCT_ALIVE provider={} receipt={}",
                plan.provider,
                receipt_path.display()
            );
            Ok(())
        }
        "BUILD_BROKEN" => Err(format!(
            "BUILD_BROKEN:GGEN_SYNC_RUN:exit={:?}:receipt={}",
            exit_code,
            receipt_path.display()
        )),
        _ => Err(format!(
            "BLOCKED:GGEN_EXECUTION:{}:receipt={}",
            error.unwrap_or_else(|| "unknown execution error".to_owned()),
            receipt_path.display()
        )),
    }
}

fn actuate(args: &CloudActuateCommand) -> Result<(), String> {
    let operation = args.operation.trim();
    if operation.is_empty() {
        return Err("REFUSED:BRCE_OPERATION_EMPTY".to_owned());
    }

    let construction: ConstructionReceipt = read_json(&args.construction)?;
    if construction.schema != CONSTRUCTION_SCHEMA {
        return Err(format!(
            "REFUSED:CONSTRUCTION_SCHEMA:expected={CONSTRUCTION_SCHEMA}:actual={}",
            construction.schema
        ));
    }
    if construction.phase != "CONSTRUCT"
        || construction.standing != "ALIVE"
        || construction.ggen_exit_code != Some(0)
    {
        return Err(format!(
            "REFUSED:CONSTRUCTION_NOT_ALIVE:standing={}:exit={:?}",
            construction.standing, construction.ggen_exit_code
        ));
    }

    ensure_new_path(&args.receipt, "BRCE_RECEIPT_PATH_EXISTS")?;
    let intent_path = args
        .intent
        .clone()
        .unwrap_or_else(|| intent_path_for(&args.receipt));
    ensure_new_path(&intent_path, "BRCE_INTENT_PATH_EXISTS")?;

    // Validate that the receipt directory is writable before crossing DO.
    preflight_receipt_sink(&args.receipt)?;

    let intent = BrceIntent {
        schema: BRCE_INTENT_SCHEMA.to_owned(),
        authority_boundary: "BRCE".to_owned(),
        operation: operation.to_owned(),
        subject: construction.subject.clone(),
        construction_receipt: path_to_display(&args.construction),
        workspace: construction.workspace.clone(),
        ggen_config: construction.ggen_config.clone(),
    };
    write_json_new(&intent_path, &intent)?;

    let mut child = Command::new(&args.brce_runner)
        .args(&args.brce_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "BLOCKED:BRCE_RUNNER_SPAWN:{}:{error}",
                args.brce_runner.display()
            )
        })?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "BLOCKED:BRCE_RUNNER_STDIN".to_owned())?;
        serde_json::to_writer(&mut stdin, &intent)
            .map_err(|error| format!("BLOCKED:BRCE_INTENT_SERIALIZE:{error}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|error| format!("BLOCKED:BRCE_INTENT_WRITE:{error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("BLOCKED:BRCE_RUNNER_WAIT:{error}"))?;
    let raw: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "REFUSED:BRCE_RECEIPT_INVALID_JSON:{error}:stderr={}",
            capture(&output.stderr)
        )
    })?;
    let receipt: BrceReceipt = serde_json::from_value(raw.clone())
        .map_err(|error| format!("REFUSED:BRCE_RECEIPT_SCHEMA:{error}"))?;

    validate_brce_receipt(&receipt, &intent)?;
    write_json_new(&args.receipt, &raw)?;

    if !output.status.success() {
        return Err(format!(
            "BLOCKED:BRCE_RUNNER_EXIT:exit={:?}:standing={}:receipt={}",
            output.status.code(),
            receipt.standing,
            args.receipt.display()
        ));
    }
    if receipt.standing != "ALIVE" {
        return Err(format!(
            "REFUSED:BRCE_STANDING:{}:receipt={}",
            receipt.standing,
            args.receipt.display()
        ));
    }

    println!(
        "DO_ALIVE provider={} operation={} receipt_id={} receipt={}",
        receipt.subject.provider,
        receipt.operation,
        receipt.receipt_id,
        args.receipt.display()
    );
    Ok(())
}

fn validate_brce_receipt(receipt: &BrceReceipt, intent: &BrceIntent) -> Result<(), String> {
    if receipt.schema != BRCE_RECEIPT_SCHEMA {
        return Err(format!(
            "REFUSED:BRCE_RECEIPT_VERSION:expected={BRCE_RECEIPT_SCHEMA}:actual={}",
            receipt.schema
        ));
    }
    if receipt.receipt_id.trim().is_empty() {
        return Err("REFUSED:BRCE_RECEIPT_ID_EMPTY".to_owned());
    }
    if receipt.actuator.trim().is_empty() {
        return Err("REFUSED:BRCE_ACTUATOR_IDENTITY_EMPTY".to_owned());
    }
    if receipt.operation != intent.operation {
        return Err(format!(
            "REFUSED:BRCE_OPERATION_MISMATCH:expected={}:actual={}",
            intent.operation, receipt.operation
        ));
    }
    if receipt.subject != intent.subject {
        return Err("REFUSED:BRCE_SUBJECT_MISMATCH".to_owned());
    }
    if !receipt.authority.is_object() {
        return Err("REFUSED:BRCE_AUTHORITY_BINDING_MISSING".to_owned());
    }
    if !receipt.consequence.is_object() {
        return Err("REFUSED:BRCE_CONSEQUENCE_BINDING_MISSING".to_owned());
    }
    if !receipt.replay.is_object() {
        return Err("REFUSED:BRCE_REPLAY_BINDING_MISSING".to_owned());
    }
    Ok(())
}

fn verify_plan(plan: &CloudPlan, python: &Path) -> Result<PathBuf, String> {
    if plan.schema != PLAN_SCHEMA {
        return Err(format!(
            "REFUSED:CLOUD_PLAN_SCHEMA:expected={PLAN_SCHEMA}:actual={}",
            plan.schema
        ));
    }
    if plan.provider.trim().is_empty() || plan.packs.is_empty() {
        return Err("REFUSED:CLOUD_PLAN_INCOMPLETE".to_owned());
    }

    let root = canonical_dir(Path::new(&plan.marketplace.root), "MARKETPLACE_ROOT")?;
    let snapshot = load_marketplace(&root, python)?;
    if snapshot.catalog_schema != plan.marketplace.catalog_schema {
        return Err("REFUSED:MARKETPLACE_CATALOG_SCHEMA_DRIFT".to_owned());
    }
    if snapshot.version != plan.marketplace.version {
        return Err(format!(
            "REFUSED:MARKETPLACE_VERSION_DRIFT:expected={}:actual={}",
            plan.marketplace.version, snapshot.version
        ));
    }
    if snapshot.fingerprint != plan.marketplace.fingerprint {
        return Err(format!(
            "REFUSED:MARKETPLACE_FINGERPRINT_DRIFT:expected={}:actual={}",
            plan.marketplace.fingerprint, snapshot.fingerprint
        ));
    }

    let available = snapshot
        .packs
        .into_iter()
        .map(|pack| (pack.name.clone(), pack))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for expected in &plan.packs {
        if !seen.insert(expected.name.clone()) {
            return Err(format!(
                "REFUSED:DUPLICATE_PACK_IN_PLAN:{}",
                expected.name
            ));
        }
        let actual = available.get(&expected.name).ok_or_else(|| {
            format!("REFUSED:PLANNED_PACK_MISSING:{}", expected.name)
        })?;
        if actual != expected {
            return Err(format!("REFUSED:PACK_IDENTITY_DRIFT:{}", expected.name));
        }
    }
    Ok(root)
}

fn subject_from_plan(plan: &CloudPlan) -> CloudSubject {
    CloudSubject {
        provider: plan.provider.clone(),
        marketplace_fingerprint: plan.marketplace.fingerprint.clone(),
        marketplace_version: plan.marketplace.version.clone(),
        packs: plan.packs.clone(),
    }
}

fn load_marketplace(root: &Path, python: &Path) -> Result<MarketplaceSnapshot, String> {
    run_marketplace(root, python, "validate")?;
    let catalog_output = run_marketplace(root, python, "catalog")?;
    let catalog: Catalog = serde_json::from_str(&catalog_output)
        .map_err(|error| format!("REFUSED:MARKETPLACE_CATALOG_JSON:{error}"))?;
    let fingerprint = run_marketplace(root, python, "fingerprint")?
        .trim()
        .to_owned();
    if !fingerprint.starts_with("sha256:") || !fingerprint.contains(" files=") {
        return Err(format!(
            "REFUSED:MARKETPLACE_FINGERPRINT_FORMAT:{fingerprint}"
        ));
    }
    Ok(MarketplaceSnapshot {
        catalog_schema: catalog.schema,
        version: catalog.marketplace_version,
        fingerprint,
        packs: catalog.packs.into_iter().map(Into::into).collect(),
    })
}

fn run_marketplace(root: &Path, python: &Path, action: &str) -> Result<String, String> {
    let script = root.join("scripts/marketplace.py");
    if !script.is_file() {
        return Err(format!(
            "REFUSED:MARKETPLACE_SCRIPT_MISSING:{}",
            script.display()
        ));
    }
    let output = Command::new(python)
        .arg(&script)
        .arg(action)
        .current_dir(root)
        .output()
        .map_err(|error| {
            format!(
                "BLOCKED:MARKETPLACE_{action}_SPAWN:{}:{error}",
                python.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "REFUSED:MARKETPLACE_{}:exit={:?}:stderr={}",
            action.to_ascii_uppercase(),
            output.status.code(),
            capture(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("REFUSED:MARKETPLACE_{action}_UTF8:{error}"))
}

fn render_ggen_config(plan: &CloudPlan, marketplace_root: &Path) -> Result<String, String> {
    let mut config = String::from(
        "# Generated by `weaver cloud construct`.\n# SELECT/CONSTRUCT only: this file grants no cloud DO authority.\n\n[packs]\n",
    );
    for pack in &plan.packs {
        let absolute = canonical_dir(&marketplace_root.join(&pack.path), "PACK_PATH")?;
        if !absolute.starts_with(marketplace_root) {
            return Err(format!("REFUSED:PACK_PATH_ESCAPE:{}", pack.name));
        }
        let key = serde_json::to_string(&pack.name)
            .map_err(|error| format!("REFUSED:PACK_NAME_TOML:{error}"))?;
        let path = path_to_utf8(&absolute, "PACK_PATH_UTF8")?;
        let quoted_path = serde_json::to_string(&path)
            .map_err(|error| format!("REFUSED:PACK_PATH_TOML:{error}"))?;
        config.push_str(&format!("{key} = {{ path = {quoted_path} }}\n"));
    }
    Ok(config)
}

fn canonical_dir(path: &Path, code: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("BLOCKED:{code}:{}:{error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("REFUSED:{code}_NOT_DIRECTORY:{}", canonical.display()));
    }
    Ok(canonical)
}

fn path_to_utf8(path: &Path, code: &str) -> Result<String, String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("REFUSED:{code}:{}", path.display()))
}

fn path_to_display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn capture(bytes: &[u8]) -> String {
    let end = bytes.len().min(MAX_CAPTURE_BYTES);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    if bytes.len() > MAX_CAPTURE_BYTES {
        text.push_str("\n...[truncated by Weaver]");
    }
    text
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("BLOCKED:READ_JSON:{}:{error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("REFUSED:JSON_INVALID:{}:{error}", path.display()))
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("BLOCKED:CREATE_PARENT:{}:{error}", parent.display())
            })?;
        }
    }
    Ok(())
}

fn ensure_new_path(path: &Path, code: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("REFUSED:{code}:{}", path.display()));
    }
    ensure_parent(path)
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    ensure_parent(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("BLOCKED:CREATE_JSON:{}:{error}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| format!("BLOCKED:WRITE_JSON:{}:{error}", path.display()))?;
    file.write_all(b"\n")
        .map_err(|error| format!("BLOCKED:WRITE_JSON:{}:{error}", path.display()))
}

fn write_new_or_identical(path: &Path, bytes: &[u8]) -> Result<(), String> {
    ensure_parent(path)?;
    if path.exists() {
        let current = fs::read(path)
            .map_err(|error| format!("BLOCKED:READ_EXISTING:{}:{error}", path.display()))?;
        if current == bytes {
            return Ok(());
        }
        return Err(format!(
            "REFUSED:WORKSPACE_CONFIG_CONFLICT:{}",
            path.display()
        ));
    }
    fs::write(path, bytes)
        .map_err(|error| format!("BLOCKED:WRITE_GGEN_CONFIG:{}:{error}", path.display()))
}

fn preflight_receipt_sink(path: &Path) -> Result<(), String> {
    ensure_parent(path)?;
    let mut reservation = path.to_path_buf();
    let name = path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "weaver-brce-receipt".into());
    let mut reserve_name = name;
    reserve_name.push(".reserve");
    reservation.set_file_name(reserve_name);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&reservation)
        .map_err(|error| {
            format!(
                "BLOCKED:BRCE_RECEIPT_SINK_PREFLIGHT:{}:{error}",
                reservation.display()
            )
        })?;
    drop(file);
    fs::remove_file(&reservation).map_err(|error| {
        format!(
            "BLOCKED:BRCE_RECEIPT_SINK_CLEANUP:{}:{error}",
            reservation.display()
        )
    })
}

fn intent_path_for(receipt: &Path) -> PathBuf {
    let mut path = receipt.to_path_buf();
    let stem = receipt
        .file_stem()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "weaver-actuation".into());
    let mut name = stem;
    name.push(".intent.json");
    path.set_file_name(name);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_path_is_adjacent_to_receipt() {
        assert_eq!(
            intent_path_for(Path::new("out/apply-receipt.json")),
            PathBuf::from("out/apply-receipt.intent.json")
        );
    }

    #[test]
    fn subject_preserves_exact_pack_identity() {
        let pack = PackIdentity {
            name: "azure-terraform-pack".to_owned(),
            version: "0.3.0".to_owned(),
            profile: "project".to_owned(),
            path: "packs/azure-terraform-pack".to_owned(),
            digest: "sha256:abc".to_owned(),
            manifest_sha256: "def".to_owned(),
        };
        let plan = CloudPlan {
            schema: PLAN_SCHEMA.to_owned(),
            provider: "azure".to_owned(),
            marketplace: MarketplaceIdentity {
                root: "/marketplace".to_owned(),
                catalog_schema: "catalog-v2".to_owned(),
                version: "1".to_owned(),
                fingerprint: "sha256:xyz files=1".to_owned(),
            },
            packs: vec![pack.clone()],
        };
        let subject = subject_from_plan(&plan);
        assert_eq!(subject.provider, "azure");
        assert_eq!(subject.packs, vec![pack]);
        assert_eq!(subject.marketplace_fingerprint, "sha256:xyz files=1");
    }
}