mock_provider "google" {}
variables { project_id = "g5-qa-1ccc" }
run "only_actual_provisioning_permissions" {
  command = plan
  assert {
    condition = toset(google_project_iam_custom_role.resources.permissions) == toset([
      "compute.disks.create", "compute.disks.use", "compute.firewalls.create", "compute.firewalls.get", "compute.firewalls.update",
      "compute.instances.create", "compute.instances.delete", "compute.instances.get",
      "compute.instances.setLabels", "compute.instances.setMetadata", "compute.instances.setServiceAccount", "compute.instances.setTags",
    ])
    error_message = "Host insertion and repair need typed create/setter permissions, with no unrelated disk/VM/firewall administration."
  }
  assert {
    condition = toset(google_project_iam_custom_role.support.permissions) == toset([
      "compute.globalOperations.get", "compute.regionOperations.get", "compute.zoneOperations.get",
      "compute.images.get", "compute.images.getFromFamily", "compute.images.useReadOnly", "compute.instances.list", "compute.zones.list",
      "compute.networks.create", "compute.networks.get", "compute.networks.updatePolicy",
      "compute.routers.create", "compute.routers.get", "compute.subnetworks.create", "compute.subnetworks.get", "compute.subnetworks.use",
    ])
    error_message = "Fallback/reconciliation need list and operation reads; network support must not grow unused update/delete/list authority."
  }
}
