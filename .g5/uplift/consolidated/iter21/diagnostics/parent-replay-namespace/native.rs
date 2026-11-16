#[path="../../../../Verus_verification/consolidated/iter21/temporal.rs"] mod model;
use model::*;
use vstd::prelude::*;
verus! {
pub proof fn observations() {
snapshot_neighbor();stable_neighbor();stale_neighbor();refresh_neighbor();readers_neighbor();reveal_with_fuel(p::materialize,2);reveal_with_fuel(p::fold,2);assert(version(Version::One)!=version(Version::Two)) by(compute);
assert((b(requested_version_preserved(stable_one(),Version::One))) == 1); // stable-requested-version-preserved
assert((b(requested_version_preserved(stale_one(),Version::One))) == 0); // stale-requested-version-preserved
assert((b(requested_version_preserved(new_two(),Version::Two))) == 1); // new-requested-version-preserved
assert((cache_version(initial())) == 1); // initial-cache-version
assert((cache_manifest_version(initial())) == 1); // initial-cache-manifest-version
assert((cache_marker(initial())) == 1); // initial-cache-marker
assert((b(match cache_row(initial()){Some(r)=>r.subdir==active(),None=>false})) == 1); // initial-cache-directory
assert((initial().cache.cache.rows.dom().len()) == 1); // initial-cache-count
assert((service_version(initial())) == 1); // initial-service-version
assert((service_marker(initial())) == 1); // initial-service-marker
assert((b(service_directory(initial())==Some(root()+active()+p::slash()))) == 1); // initial-service-directory
assert((physical_version(initial())) == 1); // initial-physical-version
assert((physical_marker(initial())) == 1); // initial-physical-marker
assert((b(listed_version(initial(),Version::One))) == 1); // initial-search-v1
assert((b(listed_version(initial(),Version::Two))) == 0); // initial-search-v2
assert((cache_version(after_edit())) == 1); // stale-cache-version
assert((cache_manifest_version(after_edit())) == 1); // stale-cache-manifest-version
assert((cache_marker(after_edit())) == 1); // stale-cache-marker
assert((b(match cache_row(after_edit()){Some(r)=>r.subdir==active(),None=>false})) == 1); // stale-cache-directory
assert((after_edit().cache.cache.rows.dom().len()) == 1); // stale-cache-count
assert((service_version(after_edit())) == 1); // stale-service-version
assert((service_marker(after_edit())) == 1); // stale-service-marker
assert((b(service_directory(after_edit())==Some(root()+active()+p::slash()))) == 1); // stale-service-directory
assert((physical_version(after_edit())) == 2); // stale-physical-version
assert((physical_marker(after_edit())) == 2); // stale-physical-marker
assert((b(listed_version(after_edit(),Version::One))) == 1); // stale-search-v1
assert((b(listed_version(after_edit(),Version::Two))) == 0); // stale-search-v2
assert((cache_version(after_refresh())) == 2); // refreshed-cache-version
assert((cache_manifest_version(after_refresh())) == 2); // refreshed-cache-manifest-version
assert((cache_marker(after_refresh())) == 2); // refreshed-cache-marker
assert((b(match cache_row(after_refresh()){Some(r)=>r.subdir==active(),None=>false})) == 1); // refreshed-cache-directory
assert((after_refresh().cache.cache.rows.dom().len()) == 1); // refreshed-cache-count
assert((service_version(after_refresh())) == 2); // refreshed-service-version
assert((service_marker(after_refresh())) == 2); // refreshed-service-marker
assert((b(service_directory(after_refresh())==Some(root()+active()+p::slash()))) == 1); // refreshed-service-directory
assert((physical_version(after_refresh())) == 2); // refreshed-physical-version
assert((physical_marker(after_refresh())) == 2); // refreshed-physical-marker
assert((b(listed_version(after_refresh(),Version::One))) == 0); // refreshed-search-v1
assert((b(listed_version(after_refresh(),Version::Two))) == 1); // refreshed-search-v2
assert((b(stable_one().world.module.is_some())) == 1); // stable-installed
assert((installed_version(stable_one().world)) == 1); // stable-installed-version
assert((installed_marker(stable_one().world)) == 1); // stable-installed-marker
assert((b(match stable_one().world.module{Some(m)=>m.description==p::source_a(),None=>false})) == 1); // stable-installed-description-exact
assert((b(installed_root(stable_one().world)==Some(root()+active()+p::slash()))) == 1); // stable-installed-root
assert((b(supplier(stable_one().world)==repository())) == 1); // stable-installed-supplier
assert((payload(stable_one().world)) == 101); // stable-payload
assert((b(shown_version(stable_one().world,Version::One))) == 1); // stable-show-v1
assert((b(shown_version(stable_one().world,Version::Two))) == 0); // stable-show-v2
assert((b(stale_one().world.module.is_some())) == 1); // stale-installed
assert((installed_version(stale_one().world)) == 2); // stale-installed-version
assert((installed_marker(stale_one().world)) == 2); // stale-installed-marker
assert((b(match stale_one().world.module{Some(m)=>m.description==p::source_b(),None=>false})) == 1); // stale-installed-description-exact
assert((b(installed_root(stale_one().world)==Some(root()+active()+p::slash()))) == 1); // stale-installed-root
assert((b(supplier(stale_one().world)==repository())) == 1); // stale-installed-supplier
assert((payload(stale_one().world)) == 202); // stale-payload
assert((b(shown_version(stale_one().world,Version::One))) == 0); // stale-show-v1
assert((b(shown_version(stale_one().world,Version::Two))) == 1); // stale-show-v2
assert((b(refused_one().world.module.is_some())) == 1); // refused-installed
assert((installed_version(refused_one().world)) == 2); // refused-installed-version
assert((installed_marker(refused_one().world)) == 2); // refused-installed-marker
assert((b(match refused_one().world.module{Some(m)=>m.description==p::source_b(),None=>false})) == 1); // refused-installed-description-exact
assert((b(installed_root(refused_one().world)==Some(root()+active()+p::slash()))) == 1); // refused-installed-root
assert((b(supplier(refused_one().world)==repository())) == 1); // refused-installed-supplier
assert((payload(refused_one().world)) == 202); // refused-payload
assert((b(shown_version(refused_one().world,Version::One))) == 0); // refused-show-v1
assert((b(shown_version(refused_one().world,Version::Two))) == 1); // refused-show-v2
assert((b(new_two().world.module.is_some())) == 1); // new-installed
assert((installed_version(new_two().world)) == 2); // new-installed-version
assert((installed_marker(new_two().world)) == 2); // new-installed-marker
assert((b(match new_two().world.module{Some(m)=>m.description==p::source_b(),None=>false})) == 1); // new-installed-description-exact
assert((b(installed_root(new_two().world)==Some(root()+active()+p::slash()))) == 1); // new-installed-root
assert((b(supplier(new_two().world)==repository())) == 1); // new-installed-supplier
assert((payload(new_two().world)) == 202); // new-payload
assert((b(shown_version(new_two().world,Version::One))) == 0); // new-show-v1
assert((b(shown_version(new_two().world,Version::Two))) == 1); // new-show-v2
assert((b(stable_one().ok)) == 1); // install-stable
assert((b(stale_one().ok)) == 1); // install-stale
assert((b(refused_one().ok)) == 0); // install-old-after-refresh
assert((b(new_two().ok)) == 1); // install-new-after-refresh
assert((b(after_edit().module.is_some())) == 1); // edited-installed
assert((installed_version(after_edit())) == 1); // edited-installed-version
assert((installed_marker(after_edit())) == 1); // edited-installed-marker
assert((b(match after_edit().module{Some(m)=>m.description==p::source_a(),None=>false})) == 1); // edited-installed-description-exact
assert((b(installed_root(after_edit())==Some(root()+active()+p::slash()))) == 1); // edited-installed-root
assert((b(supplier(after_edit())==repository())) == 1); // edited-installed-supplier
assert((payload(after_edit())) == 101); // edited-payload
assert((b(shown_version(after_edit(),Version::One))) == 1); // edited-show-v1
assert((b(shown_version(after_edit(),Version::Two))) == 0); // edited-show-v2
}
}
fn main() {}
