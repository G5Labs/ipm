# Delegated application DNS and certificate preparation

**Option B is implemented in code.** #703 preserves QA and supplies shared DNS
record management; target bootstrap/zone composition is supplied by the separate
DNS PR #707. No cloud apply, parent NS change, environment activation or merge has
been performed. Two human DNS reviews and the operational checks below remain
required before execution. The former QA zone-retirement procedure is superseded.

Cloudflare retains `g5labs.ai`, marketing and company email. It delegates three
application children to Google Cloud DNS; Terraform manages A, preview wildcard A
and Certificate Manager validation CNAMEs inside each child. There is no
Cloudflare provider, token or automated parent-zone write.

| Child zone | GCP project | Terraform owner |
| --- | --- | --- |
| `qa.g5labs.ai` | `g5-qa-1ccc` | Existing QA foundation zone; existing platform networking records |
| `lean.g5labs.ai` | `g5lean-staging-1ccc` | Existing target platform state: zone and shared networking records |
| `app.g5labs.ai` | `g5lean-prod-1ccc` | Existing target platform state: zone and shared networking records |

Bootstrap owns the required API/IAM. #707 enables target DNS and grants the
existing central release account `roles/dns.admin`; the planner keeps its existing
read-only grants. Their built-in permissions cover DNS metadata reads, with no
new DNS write grant. Verify effective permissions before execution. No new root
or identity is introduced.

Target child zones are protected and independent of ingress enablement. A configured
app/preview pair creates DNS/certificate resources even with `external_access_enabled=false`;
empty hostname configuration creates the zone only. Disabling ingress retains
configured DNS/TLS resources. Removing/changing names is a reviewed DNS/TLS change,
not an ingress-off operation. Names must remain within the environment's child.

Read `terraform output -json dns_delegation` from the owning root:

| Root | Output shape |
| --- | --- |
| QA `foundation` | `name`, `type`, `ttl`, `rrdatas` (the assigned NS targets) |
| Staging/prod `platform` | `project_id`, `zone_name`, `dns_name`, `name_servers` |

Use the actual returned NS values; never guess them. `dns_records_to_publish` is
now an inventory of Terraform-managed child records, not a request to copy A/CNAME
records into Cloudflare.

## QA preservation and delegation

Read-only inspection on 2026-10-22 found Cloudflare authoritative for QA, with no
child delegation; the dormant `g5-lean-qa` Google zone already mirrors the app A,
preview wildcard A and two validation CNAMEs. QA IP is `8.232.88.191`, its managed
certificate is ACTIVE and its certificate map is attached to the HTTPS proxy.
Recheck live values before execution. Preserve the zone and record addresses,
DNS API/runner permissions and all IP/TLS identities. No state removal/import or
resource retirement is needed for this design. Retain these exact state addresses:

```text
foundation: google_dns_managed_zone.qa
platform: module.networking.google_dns_record_set.ingress["qa.g5labs.ai"]
platform: module.networking.google_dns_record_set.preview_wildcard["preview.qa.g5labs.ai"]
platform: module.networking.google_dns_record_set.preview_dns_auth_cname["qa.g5labs.ai"]
platform: module.networking.google_dns_record_set.preview_dns_auth_cname["preview.qa.g5labs.ai"]
```

1. Review the Option B code and refreshed QA plan first. The plan
   must show no DNS/IP/TLS deletion or replacement. Reject any leftover cleanup
   or unrelated changes; retain the normal destroy guard and root order.
2. Back up state versions and export the full Cloudflare/Google DNS inventory to
   restricted operator storage. State can contain credentials; never attach it
   to a PR. Inventory every record under the proposed child, not just the four
   known records. Check CAA, DS/DNSSEC and TTLs; stop for unaccounted records or
   signing/delegation inconsistencies. Do not change apex DNSSEC. Query child DS
   at both Cloudflare nameservers: an unsigned child requires no DS; an unexpected
   existing DS stops the change. Publish a DS only for a verified signed child,
   using its live signing key.
3. Verify the intended records directly against every Google-assigned nameserver.
   The child must serve matching app/preview IPs and validation CNAMEs before any
   parent switch. Keep the existing certificate and proxy attachment working.
4. After two human DNS reviews, perform a controlled Cloudflare parent-record to
   child-NS switch. Cloudflare may reject A/AAAA/CNAME and NS at the same owner:
   plan the conflicting-record removal and delegation as a coordinated change,
   with the exported records ready for rollback. Do not assume both can coexist.
   Agree a switch window that accounts for a possible gap and negative caching;
   neither manual edits nor a batch guarantee atomic public propagation. Record
   parent delegation NS, child apex NS and parent SOA negative-cache TTLs. Remove
   superseded parent copies below the child after verifying delegation, retaining
   their export for recovery; they are not an automatic fallback.
   Preview is beneath the app child and needs no separate NS delegation. Leave
   apex/www, email, registrar nameservers and unrelated G5 records untouched.
5. Verify referrals from both Cloudflare nameservers, recursive answers and
   traces, both validation CNAMEs, ACTIVE certificate/map binding, QA login and
   a real workspace preview over trusted HTTPS. Recheck after the observed
   delegation and negative-cache TTLs. Stop for wrong referrals, SERVFAIL,
   unexpected IPs or TLS failures before continuing to staging, then production.

## Recovery

For a delegation failure, coordinate removal of the child NS delegation and
restoration of the exported Cloudflare records. Keep Google child records and
TLS resources serving the same values while cached referrals expire. Recovery
is subject to parent delegation NS, child apex NS and negative-cache TTLs,
not instantaneous. Never delete the child zone,
detach the certificate map or restore Terraform state as a DNS rollback.

## New environment activation

Use #707's target composition after the QA pilot. Keep ingress disabled while
applying configured app/preview names to platform and addons; platform creates the
protected zone, app/preview/validation records and certificate resources. Validate
records directly, then publish only the reviewed parent NS delegation. Use the
same inventory, DNSSEC, TTL and recovery checks as QA. Wait for ACTIVE certificates,
then deploy the reviewed public configuration and attach the map to its actual
HTTPS proxy. Keep promotion off until DNS, TLS and application acceptance pass;
existing customer routing changes only in the separate migration cutover.

**Automatic DNS records do not establish working HTTPS.** Follow the existing
[G5 manual certificate-map attachment procedure](../../../qa/README.md#per-preview-wildcard-origins).
Keep the GKE controller's TLS source/annotations, wait for ACTIVE certificates,
verify the actual HTTPS proxy's map, and recheck after proxy recreation. No new
activation service or automatic map attachment is proposed.

## Required verification

Review provider-mocked coverage for app/wildcard/CNAME records, target scope, QA
address preservation and private/disabled-ingress retention. Local tests do not
verify effective IAM or public DNS. Inspect refreshed QA and target plans and
check effective planner reads/denied DNS writes before execution. Live delegation,
certificate renewal capability and app/preview HTTPS require operator acceptance;
prior Option A review evidence does not establish these results.

References: [Cloudflare child delegation](https://developers.cloudflare.com/dns/manage-dns-records/how-to/subdomains-outside-cloudflare/),
[same-name NS conflicts](https://developers.cloudflare.com/dns/manage-dns-records/troubleshooting/existing-ns-record/).
