# =============================================================================
# Networking Module — VPC, subnets, Cloud NAT, firewall rules
# =============================================================================

variable "project_id" { type = string }
variable "region" { type = string }
variable "network_name" { type = string }
variable "enable_worker_egress_firewall" {
  description = "Whether to apply node-level worker egress firewall restrictions. Disable for dedicated worker clusters and rely on pod network policy instead."
  type        = bool
  default     = true
}
variable "system_subnet_cidr" { type = string }
variable "worker_subnet_cidr" { type = string }
variable "pod_cidr" { type = string }
variable "service_cidr" { type = string }
variable "worker_pod_cidr" {
  description = "Secondary CIDR range for worker-cluster pods on the worker subnet"
  type        = string
  default     = ""
}
variable "worker_service_cidr" {
  description = "Secondary CIDR range for worker-cluster services on the worker subnet"
  type        = string
  default     = ""
}
variable "tungsten_worker_pod_cidr" {
  description = "Secondary CIDR range for Tungsten worker-cluster pods on the worker subnet. Must not overlap with worker_pod_cidr."
  type        = string
  default     = ""
}
variable "tungsten_worker_service_cidr" {
  description = "Secondary CIDR range for Tungsten worker-cluster services on the worker subnet. Must not overlap with worker_service_cidr."
  type        = string
  default     = ""
}

# -----------------------------------------------------------------------------
# VPC Network
# -----------------------------------------------------------------------------
resource "google_compute_network" "main" {
  name                    = var.network_name
  project                 = var.project_id
  auto_create_subnetworks = false
  routing_mode            = "REGIONAL"
}

# -----------------------------------------------------------------------------
# Subnets
# -----------------------------------------------------------------------------
resource "google_compute_subnetwork" "system" {
  name                     = "${var.network_name}-system"
  project                  = var.project_id
  region                   = var.region
  network                  = google_compute_network.main.id
  ip_cidr_range            = var.system_subnet_cidr
  private_ip_google_access = true

  secondary_ip_range {
    range_name    = "pods"
    ip_cidr_range = var.pod_cidr
  }

  secondary_ip_range {
    range_name    = "services"
    ip_cidr_range = var.service_cidr
  }
}

resource "google_compute_subnetwork" "worker" {
  name                     = "${var.network_name}-worker"
  project                  = var.project_id
  region                   = var.region
  network                  = google_compute_network.main.id
  ip_cidr_range            = var.worker_subnet_cidr
  private_ip_google_access = true

  dynamic "secondary_ip_range" {
    for_each = var.worker_pod_cidr != "" ? [var.worker_pod_cidr] : []
    content {
      range_name    = "worker-pods"
      ip_cidr_range = secondary_ip_range.value
    }
  }

  dynamic "secondary_ip_range" {
    for_each = var.worker_service_cidr != "" ? [var.worker_service_cidr] : []
    content {
      range_name    = "worker-services"
      ip_cidr_range = secondary_ip_range.value
    }
  }

  dynamic "secondary_ip_range" {
    for_each = var.tungsten_worker_pod_cidr != "" ? [var.tungsten_worker_pod_cidr] : []
    content {
      range_name    = "tungsten-worker-pods"
      ip_cidr_range = secondary_ip_range.value
    }
  }

  dynamic "secondary_ip_range" {
    for_each = var.tungsten_worker_service_cidr != "" ? [var.tungsten_worker_service_cidr] : []
    content {
      range_name    = "tungsten-worker-services"
      ip_cidr_range = secondary_ip_range.value
    }
  }
}

# -----------------------------------------------------------------------------
# Private IP range for Cloud SQL
# -----------------------------------------------------------------------------
resource "google_compute_global_address" "private_ip_range" {
  name          = "${var.network_name}-private-ip"
  project       = var.project_id
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.main.id

  # Auto allocation must see primary and secondary subnet ranges to avoid
  # reserving a range that conflicts with a concurrently created pod subnet.
  depends_on = [google_compute_subnetwork.system, google_compute_subnetwork.worker]
}

resource "google_service_networking_connection" "private_vpc_connection" {
  network                 = google_compute_network.main.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_range.name]
}

# -----------------------------------------------------------------------------
# Cloud NAT — outbound internet for worker pods (restricted destinations)
# -----------------------------------------------------------------------------
resource "google_compute_router" "nat_router" {
  name    = "${var.network_name}-nat-router"
  project = var.project_id
  region  = var.region
  network = google_compute_network.main.id
}

resource "google_compute_router_nat" "nat" {
  name                               = "${var.network_name}-nat"
  project                            = var.project_id
  region                             = var.region
  router                             = google_compute_router.nat_router.name
  nat_ip_allocate_option             = "AUTO_ONLY"
  source_subnetwork_ip_ranges_to_nat = "ALL_SUBNETWORKS_ALL_IP_RANGES"

  log_config {
    enable = true
    filter = "ERRORS_ONLY"
  }
}

# -----------------------------------------------------------------------------
# Firewall Rules
# -----------------------------------------------------------------------------

# Allow internal communication within the VPC
resource "google_compute_firewall" "allow_internal" {
  name    = "${var.network_name}-allow-internal"
  project = var.project_id
  network = google_compute_network.main.id

  allow {
    protocol = "tcp"
  }
  allow {
    protocol = "udp"
  }
  allow {
    protocol = "icmp"
  }

  source_ranges = [
    var.system_subnet_cidr,
    var.worker_subnet_cidr,
    var.pod_cidr,
    var.service_cidr,
  ]
}

# Deny direct internet access from worker pods
# Worker pods must use Cloud NAT for allowed destinations only
resource "google_compute_firewall" "deny_worker_egress" {
  count = var.enable_worker_egress_firewall ? 1 : 0

  name      = "${var.network_name}-deny-worker-egress"
  project   = var.project_id
  network   = google_compute_network.main.id
  direction = "EGRESS"
  priority  = 1000

  deny {
    protocol = "tcp"
  }

  target_tags        = ["g5-worker"]
  destination_ranges = ["0.0.0.0/0"]
}

# Allow worker pods to reach GCS endpoints
resource "google_compute_firewall" "allow_worker_gcs" {
  count = var.enable_worker_egress_firewall ? 1 : 0

  name      = "${var.network_name}-allow-worker-gcs"
  project   = var.project_id
  network   = google_compute_network.main.id
  direction = "EGRESS"
  priority  = 900

  allow {
    protocol = "tcp"
    ports    = ["443"]
  }

  target_tags        = ["g5-worker"]
  destination_ranges = ["199.36.153.4/30"] # Google private access ranges
}

# Allow worker pods to reach the control plane (g5-api internal)
resource "google_compute_firewall" "allow_worker_control_plane" {
  count = var.enable_worker_egress_firewall ? 1 : 0

  name      = "${var.network_name}-allow-worker-control"
  project   = var.project_id
  network   = google_compute_network.main.id
  direction = "EGRESS"
  priority  = 900

  allow {
    protocol = "tcp"
    ports    = ["443", "7500", "8080"]
  }

  target_tags        = ["g5-worker"]
  destination_ranges = [var.system_subnet_cidr]
}

# -----------------------------------------------------------------------------
# Static Global IP — reserved for the GCE Ingress load balancer
# -----------------------------------------------------------------------------
variable "create_static_ip" {
  description = "Whether to create a global static IP for the Ingress load balancer"
  type        = bool
  default     = true
}

variable "static_ip_name" {
  description = "Name of the global static IP address"
  type        = string
  default     = "g5-ip"
}
# -- DNS
variable "dns_admin_project_id" {
  description = "Project owning the delegated Cloud DNS child zone; required when records are configured."
  type        = string
  default     = ""
  nullable    = false
}

variable "dns_zone_name" {
  description = "Existing Cloud DNS managed-zone name; its authority must contain every configured record."
  type        = string
  default     = ""
  nullable    = false
  validation {
    condition = !var.create_static_ip || (
      length(var.dns_hostnames) == 0 && length(var.preview_subdomain_bases) == 0
      ) || (
      trimspace(var.dns_zone_name) != "" && trimspace(var.dns_admin_project_id) != ""
    )
    error_message = "Configured DNS records require an explicit child-zone name and owning project."
  }
}

variable "dns_hostnames" {
  description = "Fully-qualified ingress hostnames managed in the delegated Cloud DNS child zone."
  type        = list(string)
  default     = []
}

variable "preview_subdomain_bases" {
  description = <<-EOT
    Per-preview wildcard origin bases (issue #935), e.g. ["preview.dev.g5labs.ai"].
    Each base receives a wildcard A record (*.<base> -> ingress IP) and is included in
    a Certificate Manager managed certificate + certificate map alongside the
    primary dns_hostnames (GKE ManagedCertificate cannot cover wildcards, and the
    certificate map must also cover the primary dns_hostnames). Empty list
    disables preview certificates. Records and validation CNAMEs are managed in
    the delegated child zone. The GCE Ingress proxy requires an explicit
    certificate-map attachment; the certmap annotation alone is insufficient.
  EOT
  type        = list(string)
  default     = []
}

locals {
  dns_hostnames          = distinct(compact(var.dns_hostnames))
  dns_records_configured = var.create_static_ip && (length(local.dns_hostnames) > 0 || length(var.preview_subdomain_bases) > 0)
  dns_zone_suffix        = local.dns_records_configured ? trimsuffix(data.google_dns_managed_zone.application[0].dns_name, ".") : ""
}

# The count depends only on configured names, never a newly created zone's outputs.
# A target can pass its zone resource name and defer this read until creation.
# Keep the scope postcondition on this read: dependent records and authorizations
# must wait for the checked zone, including when a new target zone is being created.
data "google_dns_managed_zone" "application" {
  count   = local.dns_records_configured ? 1 : 0
  project = var.dns_admin_project_id
  name    = var.dns_zone_name

  lifecycle {
    postcondition {
      condition = alltrue([for hostname in concat(local.dns_hostnames, var.preview_subdomain_bases) :
        hostname == trimsuffix(self.dns_name, ".") || endswith(hostname, ".${trimsuffix(self.dns_name, ".")}")
      ])
      error_message = "Every application and preview hostname must belong to the configured Cloud DNS child zone."
    }
  }
}

resource "google_compute_global_address" "ingress" {
  count = var.create_static_ip ? 1 : 0

  name    = var.static_ip_name
  project = var.project_id
}

resource "google_dns_record_set" "ingress" {
  for_each = var.create_static_ip ? { for hostname in local.dns_hostnames : hostname => hostname } : {}

  project      = var.dns_admin_project_id
  managed_zone = data.google_dns_managed_zone.application[0].name
  name         = "${each.value}."
  type         = "A"
  ttl          = 300
  rrdatas      = [google_compute_global_address.ingress[0].address]
}

# -----------------------------------------------------------------------------
# Per-preview wildcard origins (issue #935) — opt-in via preview_subdomain_bases
#
# Serves each preview microVM from its own hostname
# (https://<job-id>.preview.<env-domain>/). Wildcard TLS cannot come from a GKE
# ManagedCertificate, so a Certificate Manager managed certificate is published
# through a certificate map. Terraform manages its child-zone validation records;
# the operator attaches the map to the GCE
# Ingress target HTTPS proxy. The annotation alone does not perform attachment.
# The map covers primary hostnames as well as wildcards.
# -----------------------------------------------------------------------------
locals {
  preview_wildcard_enabled = length(var.preview_subdomain_bases) > 0 && var.create_static_ip
  # Every domain that needs a DNS authorization: primary hostnames + preview
  # bases (a DNS authorization for <base> also validates *.<base>).
  preview_cert_auth_domains = local.preview_wildcard_enabled ? distinct(concat(
    local.dns_hostnames,
    var.preview_subdomain_bases,
  )) : []
  # Every hostname the certificate + map must serve.
  preview_cert_domains = local.preview_wildcard_enabled ? distinct(concat(
    local.dns_hostnames,
    [for base in var.preview_subdomain_bases : "*.${base}"],
  )) : []
}

resource "google_dns_record_set" "preview_wildcard" {
  for_each = local.preview_wildcard_enabled ? { for base in var.preview_subdomain_bases : base => base } : {}

  project      = var.dns_admin_project_id
  managed_zone = data.google_dns_managed_zone.application[0].name
  name         = "*.${each.value}."
  type         = "A"
  ttl          = 300
  rrdatas      = [google_compute_global_address.ingress[0].address]
}

# DNS authorizations — one per domain; each also validates its wildcard.
resource "google_certificate_manager_dns_authorization" "preview" {
  for_each = { for domain in local.preview_cert_auth_domains : domain => domain }

  project  = var.project_id
  location = "global"
  name     = "dnsauth-${replace(each.value, ".", "-")}"
  domain   = each.value

  depends_on = [data.google_dns_managed_zone.application]
}

resource "google_dns_record_set" "preview_dns_auth_cname" {
  # Provider-issued CNAME names/targets are unknown on first creation. Preserve
  # QA's original instance keys by iterating configured authorization domains.
  for_each = { for domain in local.preview_cert_auth_domains : domain => domain }

  project      = var.dns_admin_project_id
  managed_zone = data.google_dns_managed_zone.application[0].name
  name         = google_certificate_manager_dns_authorization.preview[each.key].dns_resource_record[0].name
  type         = google_certificate_manager_dns_authorization.preview[each.key].dns_resource_record[0].type
  ttl          = 300
  rrdatas      = [google_certificate_manager_dns_authorization.preview[each.key].dns_resource_record[0].data]

  lifecycle {
    precondition {
      condition = (
        google_certificate_manager_dns_authorization.preview[each.key].dns_resource_record[0].type == "CNAME" &&
        endswith(trimsuffix(google_certificate_manager_dns_authorization.preview[each.key].dns_resource_record[0].name, "."), ".${local.dns_zone_suffix}")
      )
      error_message = "Certificate validation must be a CNAME inside the configured Cloud DNS child zone."
    }
  }
}

# One managed certificate covering primary hostnames + preview wildcards.
resource "google_certificate_manager_certificate" "preview" {
  count = local.preview_wildcard_enabled ? 1 : 0

  project  = var.project_id
  location = "global"
  name     = "${var.static_ip_name}-preview-cert"
  managed {
    domains = local.preview_cert_domains
    dns_authorizations = [
      for auth in google_certificate_manager_dns_authorization.preview : auth.id
    ]
  }
}

resource "google_certificate_manager_certificate_map" "preview" {
  count = local.preview_wildcard_enabled ? 1 : 0

  project = var.project_id
  name    = "${var.static_ip_name}-cert-map"
}

resource "google_certificate_manager_certificate_map_entry" "preview" {
  for_each = { for domain in local.preview_cert_domains : domain => domain }

  project      = var.project_id
  name         = "entry-${replace(replace(each.value, "*", "wildcard"), ".", "-")}"
  map          = google_certificate_manager_certificate_map.preview[0].name
  hostname     = each.value
  certificates = [google_certificate_manager_certificate.preview[0].id]
}

# -----------------------------------------------------------------------------
# Outputs
# -----------------------------------------------------------------------------
output "network_id" {
  value = google_compute_network.main.id
}

output "network_name" {
  value = google_compute_network.main.name
}

output "system_subnet_id" {
  value = google_compute_subnetwork.system.id
}

output "worker_subnet_id" {
  value = google_compute_subnetwork.worker.id
}

output "private_ip_range_id" {
  value = google_compute_global_address.private_ip_range.id
}

output "private_vpc_connection_id" {
  description = "ID of the private service networking connection (for depends_on)"
  value       = google_service_networking_connection.private_vpc_connection.id
}

output "ingress_static_ip" {
  description = "Reserved global static IP for the Ingress load balancer"
  value       = var.create_static_ip ? google_compute_global_address.ingress[0].address : ""
}

output "ingress_static_ip_name" {
  description = "Name of the reserved global static IP (for Ingress annotation)"
  value       = var.create_static_ip ? google_compute_global_address.ingress[0].name : ""
}

output "dns_records_to_publish" {
  description = "Terraform-managed child records by stable key for review and DNS verification; do not duplicate them in the parent zone."
  value = merge(
    var.create_static_ip ? {
      for hostname in local.dns_hostnames : "ingress:${hostname}" => {
        name = "${hostname}."
        type = "A"
        ttl  = 300
        data = google_compute_global_address.ingress[0].address
      }
    } : {},
    local.preview_wildcard_enabled ? {
      for base in var.preview_subdomain_bases : "preview:${base}" => {
        name = "*.${base}."
        type = "A"
        ttl  = 300
        data = google_compute_global_address.ingress[0].address
      }
    } : {},
    {
      for domain, auth in google_certificate_manager_dns_authorization.preview : "authorization:${domain}" => {
        name = auth.dns_resource_record[0].name
        type = auth.dns_resource_record[0].type
        ttl  = 300
        data = auth.dns_resource_record[0].data
      }
    },
  )
}

output "preview_certificate_map_name" {
  description = "Certificate Manager map to attach explicitly to the GCE Ingress target HTTPS proxy; empty when disabled"
  value       = local.preview_wildcard_enabled ? google_certificate_manager_certificate_map.preview[0].name : ""
}
