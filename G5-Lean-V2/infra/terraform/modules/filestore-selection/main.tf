# One committed selection owns provisioning, backup sources and deployment
# inputs. The committed selection is desired state, not a live-mount switch.
# Hold delivery until the manual migration verifies the copy and bindings.
variable "environment" {
  type = string
  validation {
    condition     = contains(["qa", "staging", "prod"], var.environment)
    error_message = "Filestore selection is defined only for Lean QA, staging and prod."
  }
}

locals {
  migrations = jsondecode(file("${path.module}/../../../config/filestore-migration.json"))
  migration  = lookup(local.migrations, var.environment, { provision_zonal = false, active = "legacy" })
  common = {
    zone        = "us-central1-a"
    share_name  = "g5_repos"
    capacity_gb = 1024
    storage     = "1Ti"
  }
  profiles = {
    legacy = merge(local.common, {
      instance_name = "g5-lean-${var.environment}-repos"
      pv_name       = "g5-repos-pv"
      pvc_name      = "g5-repos-pvc"
    })
    zonal = merge(local.common, {
      instance_name = "g5-lean-${var.environment}-repos-zonal"
      pv_name       = "g5-repos-zonal-pv"
      pvc_name      = "g5-repos-zonal-pvc"
    })
  }
}

output "configuration" {
  value = {
    provision_zonal = try(local.migration.provision_zonal == true, false)
    active          = try(local.migration.active, "legacy")
    legacy          = local.profiles.legacy
    zonal           = local.profiles.zonal
    selected        = lookup(local.profiles, try(local.migration.active, "legacy"), local.profiles.legacy)
  }
  precondition {
    condition = try(
      toset(keys(local.migrations)) == toset(["qa", "staging", "prod"]) &&
      alltrue([for migration in values(local.migrations) :
        toset(keys(migration)) == toset(["provision_zonal", "active"]) &&
        contains([true, false], migration.provision_zonal) &&
        contains(["legacy", "zonal"], migration.active) &&
        (migration.active != "zonal" || migration.provision_zonal == true)
      ]), false
    )
    error_message = "The committed Filestore map must define qa/staging/prod with boolean provision_zonal and active legacy/zonal; selecting Zonal requires provisioning it."
  }
}
