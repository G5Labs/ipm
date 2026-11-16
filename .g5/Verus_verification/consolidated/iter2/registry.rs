//! IPM 571e04a1 registry boundaries. Hand translation, not ObjectScript verification.
//! HTTP results, credential probes, tag ordering and filesystem outcomes are ports.
use vstd::prelude::*;
verus! {

#[derive(PartialEq, Eq)]
pub enum Probe { Accepted, Rejected, Unknown }
pub open spec fn auth_status(configured: bool, transport: bool, status: int,
    token_sent: bool, basic_sent: bool, basic_challenge: bool,
    bearer_challenge: bool, endpoint: Probe) -> Probe {
    if !configured { Probe::Accepted }
    else if !transport { Probe::Unknown }
    else if 200 <= status && status < 300 { Probe::Accepted }
    else if status != 401 && status != 403 { Probe::Unknown }
    else if token_sent || (basic_sent && basic_challenge) { Probe::Rejected }
    else if bearer_challenge && basic_sent { endpoint }
    else { Probe::Unknown }
}
pub open spec fn credentials(user: bool, pass: bool, token: bool) -> bool { user || pass || token }
pub open spec fn basic_credentials(user: bool, pass: bool, token: bool, basic: bool) -> bool {
    (user && pass) || (token && basic)
}
pub open spec fn token_probe(realm: bool, transport: bool, status: int) -> Probe {
    if !realm || !transport { Probe::Unknown }
    else if 200 <= status && status < 300 { Probe::Accepted }
    else if status == 401 || status == 403 { Probe::Rejected }
    else { Probe::Unknown }
}
pub open spec fn rejected_credentials(status: int, configured: bool, probe: Probe) -> bool {
    (status == 401 || status == 403) && configured && probe == Probe::Rejected
}
pub open spec fn suppress_listing_failure(registry_error: bool, status: int, rejected: bool) -> bool {
    registry_error && (status == 401 || status == 403) && !rejected
}
pub struct ServiceState { pub available_cached: bool, pub unavailable_cached: bool }
pub struct ServiceResult { pub state: ServiceState, pub available: bool, pub provider_called: bool }
pub open spec fn remote_cache(s: ServiceState, enabled: bool, client_exists: bool, probe: bool) -> ServiceResult {
    if s.unavailable_cached { ServiceResult { state:s, available:false, provider_called:false } }
    else if s.available_cached { ServiceResult { state:s, available:true, provider_called:false } }
    else { ServiceResult {
        state:ServiceState { available_cached:enabled && client_exists && probe,
            unavailable_cached:enabled && client_exists && !probe },
        available:enabled && client_exists && probe, provider_called:enabled && client_exists }
    }
}
pub open spec fn oras_cache(enabled: bool, provider_available: bool) -> bool { enabled && provider_available }
pub struct Capability { pub status_ok: bool, pub enabled: bool }
pub open spec fn capability(available: bool, provider_threw: bool, provider_enabled: bool) -> Capability {
    Capability { status_ok:available && !provider_threw, enabled:available && !provider_threw && provider_enabled }
}
pub open spec fn manager_status(available: bool, provider_ok: bool) -> bool { available && provider_ok }
pub open spec fn remote_authorized(user: bool, password: bool, token: bool) -> bool {
    (user && password) || token
}
pub open spec fn remote_publish(user: bool, password: bool, token: bool, export_ok: bool,
    transport_ok: bool, status: int) -> bool {
    remote_authorized(user,password,token) && export_ok && transport_ok && status == 200
}
pub open spec fn remote_delete_enabled(authorized: bool, transport_ok: bool, status: int, allow_contains_delete: bool) -> bool {
    authorized && transport_ok && status == 200 && allow_contains_delete
}
pub open spec fn remote_delete(authorized: bool, transport_ok: bool, status: int) -> bool {
    authorized && transport_ok && status == 200
}
pub open spec fn oras_delete_enabled() -> bool { true }
pub open spec fn delete_all(version_is_all: bool) -> bool { version_is_all }
pub open spec fn last_tag(tags: Seq<int>) -> Option<int> {
    if tags.len() == 0 { None } else { Some(tags[tags.len()-1]) }
}
pub struct Layer { pub tar: bool, pub size: int, pub digest: int }
pub open spec fn verify_layers(layers: Seq<Layer>, size: int, digest: int) -> bool
    decreases layers.len()
{
    if layers.len() == 0 { false }
    else if layers[0].tar { layers[0].size == size && layers[0].digest == digest }
    else { verify_layers(layers.drop_first(),size,digest) }
}
pub open spec fn module_manifest(has_xml: bool, trimmed_nonempty: bool) -> bool { has_xml && trimmed_nonempty }
pub open spec fn has_module(provider_tags: Seq<int>) -> bool { provider_tags.len() > 0 }
pub open spec fn tag_admitted(deployed: bool, platform_nonempty: bool) -> bool { !deployed || platform_nonempty }
pub struct PullResult { pub stream_set: bool, pub status_ok: bool, pub archive_selected: bool }
pub open spec fn pull(directory_ok: bool, provider_ok: bool, archive_found: bool, link_ok: bool,
    hash_ok: bool) -> PullResult {
    PullResult { stream_set: directory_ok && provider_ok && archive_found && link_ok,
        archive_selected:directory_ok && provider_ok && archive_found,
        status_ok:directory_ok && provider_ok && archive_found && link_ok && hash_ok }
}
pub struct PublishResult { pub remote_written: bool, pub version_assigned_in_memory: bool, pub status_ok: bool }
pub open spec fn publish_lifecycle(package_ok: bool, server_found: bool, name_valid: bool, installer_ok: bool,
    streams_ok: bool, provider_ok: bool, provider_written: bool, new_version: bool, local_export_ok: bool) -> PublishResult {
    let called = package_ok && server_found && name_valid && installer_ok && streams_ok;
    PublishResult { remote_written:called && provider_written, version_assigned_in_memory:called && provider_ok && new_version,
        status_ok:called && provider_ok && (!new_version || local_export_ok) }
}
pub open spec fn oras_publish(staging_ok: bool, deployed: bool, platform_nonempty: bool,
    provider_ok: bool, provider_written: bool, cleanup_ok: bool) -> PublishResult {
    let called = staging_ok && tag_admitted(deployed,platform_nonempty);
    PublishResult { remote_written:called && provider_written, version_assigned_in_memory:false, status_ok:called && provider_ok && cleanup_ok }
}
// Client push/delete calls run in sequence. The provider may reject a later call;
// completed writes/deletes at earlier tags are not compensated.
pub open spec fn write_prefix(store: Map<int,int>, tags: Seq<int>, value: int, completed: nat) -> Map<int,int>
    recommends completed <= tags.len()
    decreases completed
{
    if completed == 0 { store }
    else { write_prefix(store,tags,value,(completed-1) as nat).insert(tags[completed-1],value) }
}
pub open spec fn delete_prefix(store: Map<int,int>, tags: Seq<int>, completed: nat) -> Map<int,int>
    recommends completed <= tags.len()
    decreases completed
{
    if completed == 0 { store }
    else { delete_prefix(store,tags,(completed-1) as nat).remove(tags[completed-1]) }
}
pub proof fn manifest_and_platform_guards()
    ensures !module_manifest(false,false), module_manifest(true,true), !module_manifest(true,false),
        !has_module(Seq::<int>::empty()), has_module(seq![1int]),
        tag_admitted(false,false), !tag_admitted(true,false), tag_admitted(true,true),
        credentials(true,false,false), !credentials(false,false,false), !basic_credentials(true,false,false,false),
        basic_credentials(false,false,true,true), !basic_credentials(false,false,true,false),
        !manager_status(false,true), manager_status(true,true), !manager_status(true,false),
        delete_all(true), !delete_all(false),
        capability(true,false,false).status_ok, !capability(true,false,false).enabled,
        !capability(false,false,true).status_ok, capability(true,false,true).enabled
{}
pub proof fn failed_provider_may_have_written()
    ensures publish_lifecycle(true,true,true,true,true,false,true,true,true).remote_written,
        !publish_lifecycle(true,true,true,true,true,false,true,true,true).status_ok,
        oras_publish(true,false,false,false,true,true).remote_written,
        !oras_publish(true,false,false,false,true,true).status_ok
{}
pub proof fn tag_order_is_a_real_input()
    ensures last_tag(seq![2int,1]) == Some(1), last_tag(seq![1int,2]) == Some(2),
        last_tag(Seq::<int>::empty()) == None
{}
pub proof fn first_tar_controls_integrity()
    ensures !verify_layers(seq![Layer{tar:true,size:2,digest:9},Layer{tar:true,size:1,digest:7}],1,7),
        verify_layers(seq![Layer{tar:false,size:2,digest:9},Layer{tar:true,size:1,digest:7}],1,7),
        verify_layers(seq![Layer{tar:true,size:1,digest:7},Layer{tar:true,size:2,digest:9}],1,7)
{ reveal_with_fuel(verify_layers, 3); }
pub proof fn cache_changes_operator_observation()
    ensures remote_cache(ServiceState{available_cached:true,unavailable_cached:false},false,true,false).available,
        !oras_cache(false,true),
        !remote_cache(ServiceState{available_cached:false,unavailable_cached:true},true,true,true).available,
        oras_cache(true,true)
{}
pub proof fn auth_is_three_valued()
    ensures auth_status(false,false,0,false,false,false,false,Probe::Unknown) == Probe::Accepted,
        auth_status(true,true,401,false,true,true,false,Probe::Unknown) == Probe::Rejected,
        auth_status(true,true,401,false,true,false,true,Probe::Accepted) == Probe::Accepted,
        auth_status(true,true,403,false,true,false,false,Probe::Unknown) == Probe::Unknown,
        token_probe(false,true,200) == Probe::Unknown,
        token_probe(true,true,403) == Probe::Rejected
{}
pub proof fn permission_is_not_bad_credentials()
    ensures !rejected_credentials(403,true,Probe::Accepted), rejected_credentials(403,true,Probe::Rejected),
        suppress_listing_failure(true,403,false), !suppress_listing_failure(true,403,true),
        !suppress_listing_failure(true,500,false)
{}
pub proof fn remote_exact_status_contract()
    ensures remote_publish(true,true,false,true,true,200), !remote_publish(true,true,false,true,true,201),
        !remote_publish(true,false,false,true,true,200),
        remote_delete_enabled(true,true,200,true), !remote_delete_enabled(true,true,204,true),
        remote_delete(true,true,200), !remote_delete(true,true,204), oras_delete_enabled()
{}
pub proof fn failed_hash_keeps_downloaded_stream()
    ensures pull(true,true,true,true,false).stream_set, !pull(true,true,true,true,false).status_ok,
        pull(true,true,true,true,true).status_ok, !pull(true,true,false,true,true).stream_set
{}
pub proof fn post_publish_failure_is_not_compensation()
    ensures publish_lifecycle(true,true,true,true,true,true,true,true,false).remote_written,
        publish_lifecycle(true,true,true,true,true,true,true,true,false).version_assigned_in_memory,
        !publish_lifecycle(true,true,true,true,true,true,true,true,false).status_ok,
        publish_lifecycle(true,true,true,true,true,true,true,true,true).status_ok,
        oras_publish(true,false,false,true,true,false).remote_written,
        !oras_publish(true,false,false,true,true,false).status_ok
{}
pub proof fn partial_remote_effects()
    ensures write_prefix(Map::<int,int>::empty(),seq![1int,2],7,1).contains_pair(1,7),
        !write_prefix(Map::<int,int>::empty(),seq![1int,2],7,1).dom().contains(2),
        !delete_prefix(Map::<int,int>::empty().insert(1,7).insert(2,7),seq![1int,2],1).dom().contains(1),
        delete_prefix(Map::<int,int>::empty().insert(1,7).insert(2,7),seq![1int,2],1).contains_pair(2,7)
{ reveal_with_fuel(write_prefix, 3); reveal_with_fuel(delete_prefix, 3); }
}
fn main() {}
