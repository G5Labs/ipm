# Operator-applied bootstrap owns these definitions. Runtime/platform runners
# bind their fixed names and never need permission to edit custom roles.
terraform {
  required_providers {
    google = { source = "hashicorp/google" }
  }
}

variable "project_id" { type = string }

# These resource types support IAM name conditions, including VM/disk creation.
# Keep list and supporting permissions out so the binding can fail closed for
# every resource outside the host prefixes. instances.insert may reuse an
# existing same-name boot disk; scoped disks.use preserves that recovery path.
resource "google_project_iam_custom_role" "resources" {
  project     = var.project_id
  role_id     = "g5LeanWorkerHostResources"
  title       = "G5 Lean worker host resources"
  description = "Create and reconcile named host VMs, boot disks and organization firewalls. Bind with resource-name conditions."
  permissions = [
    "compute.disks.create", "compute.disks.use",
    "compute.firewalls.create", "compute.firewalls.get", "compute.firewalls.update",
    "compute.instances.create", "compute.instances.delete", "compute.instances.get",
    "compute.instances.setLabels", "compute.instances.setMetadata",
    "compute.instances.setServiceAccount", "compute.instances.setTags",
  ]
}

# Networks, subnetworks and routers do not expose IAM resource.name conditions;
# list checks use the parent project and operation names are provider-generated.
# This necessary project-level authority does NOT isolate Terraform-owned VPCs:
# the API can attach a newly named host/firewall to another existing network.
# The image resolver accepts concrete images and family refs; family lookups
# require images.getFromFamily separately from images.get.
resource "google_project_iam_custom_role" "support" {
  project     = var.project_id
  role_id     = "g5LeanWorkerHostSupport"
  title       = "G5 Lean worker host provisioning support"
  description = "Networking creation/use, image reads and provisioning discovery; project-scoped residual network authority."
  permissions = [
    "compute.globalOperations.get", "compute.regionOperations.get", "compute.zoneOperations.get",
    "compute.images.get", "compute.images.getFromFamily", "compute.images.useReadOnly",
    "compute.instances.list", "compute.zones.list",
    "compute.networks.create", "compute.networks.get", "compute.networks.updatePolicy",
    "compute.routers.create", "compute.routers.get",
    "compute.subnetworks.create", "compute.subnetworks.get", "compute.subnetworks.use",
  ]
}
