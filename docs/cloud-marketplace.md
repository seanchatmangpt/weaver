# Cloud integration with ggen Marketplace

Weaver's cloud integration is provider-neutral and capability-pack driven. The implementation deliberately separates **SELECT**, **CONSTRUCT**, and **DO** so generated artifacts never acquire ambient execution authority.

The command is experimental and therefore hidden from the top-level generated help while the BRCE runner protocol stabilizes. It is directly invokable as `weaver cloud ...`.

## Lifecycle

```text
marketplace.toml + packs/*
        |
        |  marketplace.py validate/catalog/fingerprint
        v
SELECT: weaver cloud plan
        |
        |  exact marketplace + pack identities
        v
weaver.cloud.plan.v1
        |
        |  replay admission; refuse drift
        v
CONSTRUCT: weaver cloud construct
        |
        |  ggen sync run only
        v
consumer ggen.toml + generated artifacts
        |
        |  ALIVE construction receipt required
        v
DO: weaver cloud actuate
        |
        |  versioned JSON intent over stdin
        v
explicit BRCE runner
        |
        |  versioned JSON receipt over stdout
        v
authority + consequence + replay + standing
```

Weaver does **not** directly invoke Terraform, `kubectl`, `aws`, `az`, `gcloud`, or a provider SDK. Cloud DO is delegated exclusively to an explicitly supplied BRCE runner.

## 1. SELECT an exact marketplace subject

```bash
weaver cloud plan \
  --marketplace ../ggen-marketplace \
  --provider azure \
  --pack azure-terraform-pack \
  --pack otel-weaver-pack \
  --output .weaver/azure-plan.json
```

`plan` runs the marketplace's canonical `scripts/marketplace.py validate`, `catalog`, and `fingerprint` commands. It writes a new `weaver.cloud.plan.v1` document binding:

- provider identity;
- marketplace catalog schema and marketplace version;
- whole-marketplace fingerprint;
- exact pack name and version;
- pack profile and repository path;
- deterministic pack archive digest; and
- pack manifest SHA-256.

Pack selection is explicit. The current ggen Marketplace catalog does not publish a canonical `provider -> pack set` relation, so Weaver refuses to manufacture one from pack names. When the marketplace publishes such an ontology relation, selection can become fully automatic without adding provider-specific Weaver branches.

Existing plan paths are refused rather than overwritten.

## 2. CONSTRUCT with ggen

```bash
weaver cloud construct \
  --plan .weaver/azure-plan.json \
  --workspace .weaver/cloud/azure
```

Before generation, Weaver re-runs marketplace admission and compares the current catalog version, fingerprint, and every selected pack identity with the plan. Any drift is refused.

For an admitted plan Weaver writes a dedicated consumer `ggen.toml` containing local pack references and runs exactly:

```text
ggen sync run
```

This phase is **CONSTRUCT**, not DO. The generated `ggen.toml`, templates, Terraform, Kubernetes manifests, scripts, or other projected artifacts have no cloud execution authority merely because they exist.

The default receipt is:

```text
<workspace>/weaver-construction-receipt.json
```

It records the exact cloud subject, plan, workspace, generated config, command, exit code, bounded stdout/stderr, and one of the following standing values:

- `ALIVE` — `ggen sync run` was observed to exit successfully against the exact admitted plan;
- `BUILD_BROKEN` — ggen executed but returned failure; or
- `BLOCKED` — ggen could not be executed.

An existing, non-identical workspace `ggen.toml` is refused instead of overwritten.

## 3. DO only through BRCE

```bash
weaver cloud actuate \
  --construction .weaver/cloud/azure/weaver-construction-receipt.json \
  --brce-runner ./bin/cloud-brce \
  --operation apply \
  --receipt .weaver/cloud/azure/apply-receipt.json
```

Optional runner arguments are passed without a shell:

```bash
--brce-arg value
```

`actuate` requires an `ALIVE` `weaver.cloud.construction-receipt.v1`. Before running BRCE it writes a replayable `weaver.brce.cloud-intent.v1` document beside the requested receipt unless `--intent` is supplied explicitly. Existing intent or receipt paths are refused, and the receipt directory is preflighted before crossing DO.

### BRCE runner input

The runner receives one JSON document on stdin:

```json
{
  "schema": "weaver.brce.cloud-intent.v1",
  "authority_boundary": "BRCE",
  "operation": "apply",
  "subject": {
    "provider": "azure",
    "marketplace_fingerprint": "sha256:... files=...",
    "marketplace_version": "...",
    "packs": []
  },
  "construction_receipt": ".weaver/cloud/azure/weaver-construction-receipt.json",
  "workspace": "...",
  "ggen_config": ".../ggen.toml"
}
```

The `subject.packs` array contains the complete pack identities admitted during SELECT.

### Required BRCE receipt

The runner must return one JSON document on stdout matching `weaver.brce.cloud-receipt.v1`:

```json
{
  "schema": "weaver.brce.cloud-receipt.v1",
  "receipt_id": "provider-or-runtime-specific-stable-id",
  "actuator": "exact actuator identity",
  "standing": "ALIVE",
  "operation": "apply",
  "subject": {
    "provider": "azure",
    "marketplace_fingerprint": "sha256:... files=...",
    "marketplace_version": "...",
    "packs": []
  },
  "authority": {
    "principal": "...",
    "scope": "..."
  },
  "consequence": {
    "resource_ids": []
  },
  "replay": {
    "mechanism": "..."
  }
}
```

Weaver validates all of the following before granting DO standing:

1. receipt protocol version;
2. non-empty receipt identity;
3. non-empty actuator identity;
4. exact operation echo;
5. exact admitted cloud-subject echo;
6. structured authority binding;
7. structured consequence binding;
8. structured replay binding;
9. runner process success; and
10. receipt `standing == ALIVE`.

A valid non-ALIVE receipt is preserved as evidence but does not produce DO standing. Invalid or mismatched receipts are refused. If a BRCE runner performs an external consequence but fails to return a valid receipt, that runner has falsified the BRCE contract; Weaver will not claim the actuation is ALIVE.

## Provider extension model

Provider identity is intentionally a string rather than a Rust enum. AWS, Azure, GCP, Oracle, IBM, SAP, Salesforce, private clouds, and future IaaS/PaaS/SaaS marketplaces are all handled through the same lifecycle. Provider-specific capability lives in ggen Marketplace packs and the authorized BRCE runtime, not in a growing tree of provider-specific Weaver execution paths.

The extension path is therefore:

```text
publish/admit marketplace capability pack
        -> select exact pack identity
        -> ggen construct
        -> BRCE actuate
        -> bound receipt
```

One failed provider or pack edge is topology, not failure of the cloud-integration graph.