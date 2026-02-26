$ErrorActionPreference = "Stop"

$root = Join-Path $PSScriptRoot ".."
$vectorsRoot = Join-Path $root "tests\conformance\v1\vectors"

$categories = @("core", "ref", "type", "ssa", "cost", "sec", "stall", "narr", "det")
foreach ($c in $categories) {
  New-Item -ItemType Directory -Force -Path (Join-Path $vectorsRoot $c) | Out-Null
}

function New-Handle(
  [string]$Ref,
  [string]$Availability = "READY",
  [string]$TrustTier = "TIER_3_CONFIRMED",
  [string[]]$Taint = @(),
  [string]$Subject = "user:vinz",
  [string]$Predicate = "prefers_beverage"
) {
  return @{
    ref = $Ref
    type_id = "normative.preference"
    availability = $Availability
    subject = $Subject
    predicate_label = $Predicate
    trust_tier = $TrustTier
    taint = $Taint
    scope = "SCOPE_GLOBAL"
    signature_summary = "$Predicate for $Subject"
    conflict_group_id = "conflict:$($Predicate):$($Subject):global"
    open_end = $true
  }
}

function Base-Manifest([string]$RequestId = "req-conformance") {
  return @{
    request_id = $RequestId
    handles = @(
      (New-Handle -Ref "H0")
    )
    selectors = @(
      @{
        sel = "S0"
        description = "Find preferences for subject"
        params = @(
          @{
            name = "subject"
            type = "PARAM_STRING"
            enum_values = @()
          }
        )
        cost_weight = 1.25
        return_type = "RETURN_HANDLE_SET"
      }
    )
    context = @()
    budget = @{
      max_ops = 128
      max_join_depth = 3
      max_fanout = 64
      max_total_cost = 256.0
    }
  }
}

function Base-Vector([string]$Id, [string]$Description, [string]$Status) {
  return @{
    vector_id = $Id
    spec_version = "conformance/v1.0.0"
    proto_version = "cortex_rmvm_v3_1"
    description = $Description
    manifest = (Base-Manifest -RequestId "req-$Id")
    plan = @{
      request_id = "req-$Id"
      steps = @()
      outputs = @()
    }
    execute_options = @{
      allow_partial_on_stall = $false
      degraded_mode = $false
      broken_lineage_handles = @()
      narrative_templates = @()
    }
    expect = @{
      status = $Status
      error_code = $null
      semantic_root = $null
      verified_blocks = $null
      stall = $null
    }
    determinism = @{
      assert_response_cpe_sha256 = $null
      assert_semantic_root = $true
    }
  }
}

function Save-Vector([string]$Category, [hashtable]$Vector) {
  $path = Join-Path (Join-Path $vectorsRoot $Category) "$($Vector.vector_id).json"
  $payload = ($Vector | ConvertTo-Json -Depth 30) + "`n"
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($path, $payload, $utf8NoBom)
}

# CORE
$v = Base-Vector "C31-CORE-001-happy-fetch-project-assert" "Happy path fetch/project/assert." "OK"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{
    out = "r2"
    op = @{
      kind = "assert"
      assertion_type = "ASSERT_WORLD_FACT"
      bindings = @{ subject = @{ reg = "r1"; field_path = "meta.subject" } }
      citations = @()
    }
  }
)
$v.plan.outputs = @("r2")
Save-Vector "core" $v

$v = Base-Vector "C31-CORE-002-selector-resolve-deterministic" "Selector then resolve deterministic winner." "OK"
$v.manifest.handles = @(
  (New-Handle -Ref "H0" -TrustTier "TIER_3_CONFIRMED"),
  (New-Handle -Ref "H1" -TrustTier "TIER_2_VERIFIED")
)
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } },
  @{ out = "r1"; op = @{ kind = "resolve"; in_reg = "r0"; policy_id = "default" } },
  @{ out = "r2"; op = @{ kind = "project"; in_reg = "r1"; field_paths = @("ref") } },
  @{ out = "r3"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ winner = @{ reg = "r2"; field_path = "ref" } }; citations = @() } }
)
$v.plan.outputs = @("r3")
Save-Vector "core" $v

$v = Base-Vector "C31-CORE-003-filter-by-subject" "Filter handle set by subject." "OK"
$v.manifest.handles = @(
  (New-Handle -Ref "H0" -Subject "user:vinz"),
  (New-Handle -Ref "H1" -Subject "user:other")
)
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } },
  @{ out = "r1"; op = @{ kind = "filter"; in_reg = "r0"; filter_ref = "by_subject"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } },
  @{ out = "r2"; op = @{ kind = "project"; in_reg = "r1"; field_paths = @("meta.subject") } },
  @{ out = "r3"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ subject = @{ reg = "r2"; field_path = "meta.subject" } }; citations = @() } }
)
$v.plan.outputs = @("r3")
Save-Vector "core" $v

$v = Base-Vector "C31-CORE-004-join-same-entity" "Join handles on same entity edge." "OK"
$v.manifest.handles = @(
  (New-Handle -Ref "H0" -Subject "user:vinz"),
  (New-Handle -Ref "H1" -Subject "user:vinz"),
  (New-Handle -Ref "H2" -Subject "user:other")
)
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "fetch"; handle_ref = "H1" } },
  @{ out = "r2"; op = @{ kind = "join"; left_reg = "r0"; right_reg = "r1"; edge_type = "EDGE_SAME_ENTITY" } },
  @{ out = "r3"; op = @{ kind = "project"; in_reg = "r2"; field_paths = @("set_count") } },
  @{ out = "r4"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ c = @{ reg = "r3"; field_path = "set_count" } }; citations = @() } }
)
$v.plan.outputs = @("r4")
Save-Vector "core" $v

$v = Base-Vector "C31-CORE-005-empty-assert-bindings-reject" "Empty assertion bindings are rejected." "REJECTED"
$v.expect.error_code = "UNTRUSTED_PROVENANCE"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "assert"; assertion_type = "ASSERT_DECISION"; bindings = @{}; citations = @() } }
)
Save-Vector "core" $v

# REF
$v = Base-Vector "C31-REF-001-unknown-handle-ref" "Unknown handle ref should reject." "REJECTED"
$v.expect.error_code = "UNKNOWN_HANDLE_REF"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H404" } }
)
Save-Vector "ref" $v

$v = Base-Vector "C31-REF-002-unknown-selector-ref" "Unknown selector ref should reject." "REJECTED"
$v.expect.error_code = "UNKNOWN_SELECTOR_REF"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S404"; params = @{} } }
)
Save-Vector "ref" $v

# TYPE
$v = Base-Vector "C31-TYPE-001-selector-param-type-mismatch" "Selector param type mismatch should reject." "REJECTED"
$v.expect.error_code = "TYPE_MISMATCH"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "b"; value = $true } } } }
)
Save-Vector "type" $v

$v = Base-Vector "C31-TYPE-002-project-unknown-field" "Project unknown field should reject." "REJECTED"
$v.expect.error_code = "FIELD_REDACTED"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.unknown") } }
)
Save-Vector "type" $v

$v = Base-Vector "C31-TYPE-003-join-non-handle-input" "Join non-handle regs should reject." "REJECTED"
$v.expect.error_code = "TYPE_MISMATCH"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{ out = "r2"; op = @{ kind = "join"; left_reg = "r1"; right_reg = "r0"; edge_type = "EDGE_SAME_ENTITY" } }
)
Save-Vector "type" $v

# SSA
$v = Base-Vector "C31-SSA-001-register-redefinition" "Register redefinition should reject." "REJECTED"
$v.expect.error_code = "SCHEMA_VIOLATION"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r0"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } }
)
Save-Vector "ssa" $v

$v = Base-Vector "C31-SSA-002-use-before-define" "Use-before-define should reject." "REJECTED"
$v.expect.error_code = "SCHEMA_VIOLATION"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "project"; in_reg = "r9"; field_paths = @("meta.subject") } }
)
Save-Vector "ssa" $v

$v = Base-Vector "C31-SSA-003-register-smuggling-value-ref" "Assert binding from non-fetch/non-project register should reject." "REJECTED"
$v.expect.error_code = "SCHEMA_VIOLATION"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } },
  @{ out = "r1"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ x = @{ reg = "r0"; field_path = "ref" } }; citations = @() } }
)
Save-Vector "ssa" $v

# COST
$v = Base-Vector "C31-COST-001-wide-select-cost-bypass" "Wide select should be rejected by cost guard." "REJECTED"
$v.expect.error_code = "COST_GUARD_REJECTED"
$handles = @()
for ($i = 0; $i -lt 80; $i++) { $handles += (New-Handle -Ref "H$i") }
$v.manifest.handles = $handles
$v.manifest.selectors[0].cost_weight = 10.0
$v.manifest.budget.max_total_cost = 5.0
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } }
)
Save-Vector "cost" $v

$v = Base-Vector "C31-COST-002-selector-fanout-overflow" "Selector fanout overflow should reject." "REJECTED"
$v.expect.error_code = "GRAPH_TRAVERSAL_LIMIT"
$v.manifest.handles = @(
  (New-Handle -Ref "H0"),
  (New-Handle -Ref "H1"),
  (New-Handle -Ref "H2")
)
$v.manifest.budget.max_fanout = 1
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S0"; params = @{ subject = @{ kind = "s"; value = "user:vinz" } } } }
)
Save-Vector "cost" $v

$v = Base-Vector "C31-COST-003-join-depth-overflow" "Join depth over max should reject." "REJECTED"
$v.expect.error_code = "GRAPH_TRAVERSAL_LIMIT"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r2"; op = @{ kind = "join"; left_reg = "r0"; right_reg = "r1"; edge_type = "EDGE_SAME_ENTITY" } },
  @{ out = "r3"; op = @{ kind = "join"; left_reg = "r2"; right_reg = "r0"; edge_type = "EDGE_SAME_ENTITY" } },
  @{ out = "r4"; op = @{ kind = "join"; left_reg = "r3"; right_reg = "r0"; edge_type = "EDGE_SAME_ENTITY" } },
  @{ out = "r5"; op = @{ kind = "join"; left_reg = "r4"; right_reg = "r0"; edge_type = "EDGE_SAME_ENTITY" } }
)
Save-Vector "cost" $v

$v = Base-Vector "C31-COST-004-max-ops-overflow" "Plan with too many ops should reject." "REJECTED"
$v.expect.error_code = "GRAPH_TRAVERSAL_LIMIT"
$v.manifest.budget.max_ops = 2
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{ out = "r2"; op = @{ kind = "project"; in_reg = "r1"; field_paths = @("meta.subject") } }
)
Save-Vector "cost" $v

# SEC
$v = Base-Vector "C31-SEC-001-selector-spoofing" "Selector spoofing should reject." "REJECTED"
$v.expect.error_code = "UNKNOWN_SELECTOR_REF"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "apply_selector"; selector_ref = "S_spoof"; params = @{} } }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-002-handle-spoofing" "Handle spoofing should reject." "REJECTED"
$v.expect.error_code = "UNKNOWN_HANDLE_REF"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H_spoof" } }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-003-broken-lineage-direct-citation" "Broken lineage direct citation should reject." "REJECTED"
$v.expect.error_code = "ERROR_BROKEN_LINEAGE"
$v.execute_options.broken_lineage_handles = @("H0")
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{
    out = "r2"
    op = @{
      kind = "assert"
      assertion_type = "ASSERT_WORLD_FACT"
      bindings = @{ subject = @{ reg = "r1"; field_path = "meta.subject" } }
      citations = @(@{ kind = "handle_ref"; value = "H0" })
    }
  }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-004-broken-lineage-transitive" "Broken lineage transitive propagation should reject." "REJECTED"
$v.expect.error_code = "ERROR_BROKEN_LINEAGE"
$v.execute_options.broken_lineage_handles = @("H0")
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{ out = "r2"; op = @{ kind = "project"; in_reg = "r1"; field_paths = @("meta.subject") } },
  @{ out = "r3"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ subject = @{ reg = "r2"; field_path = "meta.subject" } }; citations = @() } }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-005-trust-gate-policy-tier2-required" "Policy assertion below TIER_2 should reject." "REJECTED"
$v.expect.error_code = "UNTRUSTED_PROVENANCE"
$v.manifest.handles = @((New-Handle -Ref "H0" -TrustTier "TIER_1_ASSERTED"))
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "assert"; assertion_type = "ASSERT_DECISION"; bindings = @{ who = @{ reg = "r0"; field_path = "meta.subject" } }; citations = @() } }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-006-taint-gate-web-untrusted-policy-sink" "Web untrusted taint into policy sink should reject." "REJECTED"
$v.expect.error_code = "DATA_LEAK_PREVENTION"
$v.manifest.handles = @((New-Handle -Ref "H0" -Taint @("TAINT_WEB_UNTRUSTED")))
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "assert"; assertion_type = "ASSERT_DECISION"; bindings = @{ who = @{ reg = "r0"; field_path = "meta.subject" } }; citations = @() } }
)
Save-Vector "sec" $v

$v = Base-Vector "C31-SEC-007-taint-gate-mixed-policy-sink" "Mixed taint into policy sink should reject." "REJECTED"
$v.expect.error_code = "DATA_LEAK_PREVENTION"
$v.manifest.handles = @((New-Handle -Ref "H0" -Taint @("TAINT_MIXED")))
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "assert"; assertion_type = "ASSERT_PROCEDURE"; bindings = @{ who = @{ reg = "r0"; field_path = "meta.subject" } }; citations = @() } }
)
Save-Vector "sec" $v

# STALL
$v = Base-Vector "C31-STALL-001-offline-fetch-stall" "OFFLINE handle fetch should stall." "STALL"
$v.manifest.handles = @((New-Handle -Ref "H0" -Availability "OFFLINE"))
$v.expect.stall = @{
  handle_ref = "H0"
  availability = "OFFLINE"
  retrieval_ticket_present = $true
}
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } }
)
Save-Vector "stall" $v

$v = Base-Vector "C31-STALL-002-archival-pending-fetch-stall" "ARCHIVAL_PENDING handle fetch should stall." "STALL"
$v.manifest.handles = @((New-Handle -Ref "H0" -Availability "ARCHIVAL_PENDING"))
$v.expect.stall = @{
  handle_ref = "H0"
  availability = "ARCHIVAL_PENDING"
  retrieval_ticket_present = $true
}
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } }
)
Save-Vector "stall" $v

# NARR
$v = Base-Vector "C31-NARR-001-token-guard-unbound-number" "Narrative with unbound number should reject." "REJECTED"
$v.expect.error_code = "DATA_LEAK_PREVENTION"
$v.execute_options.narrative_templates = @("You scored 100 today.")
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } }
)
Save-Vector "narr" $v

$v = Base-Vector "C31-NARR-002-token-guard-unbound-proper-noun" "Narrative with unbound proper noun should reject." "REJECTED"
$v.expect.error_code = "DATA_LEAK_PREVENTION"
$v.execute_options.narrative_templates = @("Alice approved this policy.")
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } }
)
Save-Vector "narr" $v

# DET
$v = Base-Vector "C31-DET-001-map-order-permutation-a" "Assertion binding map order A." "OK"
$v.manifest.handles = @(
  (New-Handle -Ref "H0"),
  (New-Handle -Ref "H1" -Subject "user:vinz")
)
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject", "type_id") } },
  @{ out = "r2"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = [ordered]@{ b = @{ reg = "r1"; field_path = "type_id" }; a = @{ reg = "r1"; field_path = "meta.subject" } }; citations = @() } }
)
$v.plan.outputs = @("r2")
Save-Vector "det" $v

$v = Base-Vector "C31-DET-002-map-order-permutation-b" "Assertion binding map order B." "OK"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject", "type_id") } },
  @{ out = "r2"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = [ordered]@{ a = @{ reg = "r1"; field_path = "meta.subject" }; b = @{ reg = "r1"; field_path = "type_id" } }; citations = @() } }
)
$v.plan.outputs = @("r2")
Save-Vector "det" $v

$v = Base-Vector "C31-DET-003-citation-order-permutation-a" "Citation order permutation A." "OK"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{ out = "r2"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ subject = @{ reg = "r1"; field_path = "meta.subject" } }; citations = @(@{ kind = "anchor_ref"; value = "zeta" }, @{ kind = "anchor_ref"; value = "alpha" }) } }
)
$v.plan.outputs = @("r2")
Save-Vector "det" $v

$v = Base-Vector "C31-DET-004-citation-order-permutation-b" "Citation order permutation B." "OK"
$v.plan.steps = @(
  @{ out = "r0"; op = @{ kind = "fetch"; handle_ref = "H0" } },
  @{ out = "r1"; op = @{ kind = "project"; in_reg = "r0"; field_paths = @("meta.subject") } },
  @{ out = "r2"; op = @{ kind = "assert"; assertion_type = "ASSERT_WORLD_FACT"; bindings = @{ subject = @{ reg = "r1"; field_path = "meta.subject" } }; citations = @(@{ kind = "anchor_ref"; value = "alpha" }, @{ kind = "anchor_ref"; value = "zeta" }) } }
)
$v.plan.outputs = @("r2")
Save-Vector "det" $v

Write-Output "Generated conformance vectors under $vectorsRoot"
