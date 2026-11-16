use vstd::prelude::*;
#[path="../sync_summary.rs"] pub mod model;
use model::*;
verus!{pub proof fn observed_group(){
reveal_with_fuel(rows_from,5);reveal_with_fuel(report_from,4);staged_store();report_neighbor();selected_neighbor();caller_neighbor();
// inactive-active

assert((b((inactive()).active))==0);
// begun-active

assert((b((begun()).active))==1);
// a1-zeta-class1-exists

assert((b(has_row(a1(),zeta(),1)))==1);
// inactive-module-count

assert((module_count(inactive()) as int)==0);
// inactive-zeta-count

assert((count(inactive(),zeta()) as int)==0);
// inactive-alpha-count

assert((count(inactive(),alpha()) as int)==0);
// inactive-zeta-rows

assert((rows_from(inactive(),zeta(),1).len() as int)==0);
// inactive-zeta-class1

assert((b(matching(inactive(),zeta(),1,verify_z())))==0);
// inactive-zeta-class2

assert((b(matching(inactive(),zeta(),2,verify_a())))==0);
// inactive-zeta-class3

assert((b(matching(inactive(),zeta(),3,verify_z())))==0);
// inactive-alpha-class1

assert((b(matching(inactive(),alpha(),1,alpha_class())))==0);
// inactive-zeta-class1-exists

assert((b(has_row(inactive(),zeta(),1)))==0);
// inactive-alpha-class1-exists

assert((b(has_row(inactive(),alpha(),1)))==0);
// begun-module-count

assert((module_count(begun()) as int)==0);
// begun-zeta-count

assert((count(begun(),zeta()) as int)==0);
// begun-alpha-count

assert((count(begun(),alpha()) as int)==0);
// begun-zeta-rows

assert((rows_from(begun(),zeta(),1).len() as int)==0);
// begun-zeta-class1

assert((b(matching(begun(),zeta(),1,verify_z())))==0);
// begun-zeta-class2

assert((b(matching(begun(),zeta(),2,verify_a())))==0);
// begun-zeta-class3

assert((b(matching(begun(),zeta(),3,verify_z())))==0);
// begun-alpha-class1

assert((b(matching(begun(),alpha(),1,alpha_class())))==0);
// begun-zeta-class1-exists

assert((b(has_row(begun(),zeta(),1)))==0);
// begun-alpha-class1-exists

assert((b(has_row(begun(),alpha(),1)))==0);
// a1-active

assert((b((a1()).active))==1);
// a1-module-count

assert((module_count(a1()) as int)==1);
// a1-zeta-count

assert((count(a1(),zeta()) as int)==1);
// a1-alpha-count

assert((count(a1(),alpha()) as int)==0);
// a1-zeta-rows

assert((rows_from(a1(),zeta(),1).len() as int)==1);
// a1-zeta-class1

assert((b(matching(a1(),zeta(),1,verify_z())))==1);
// a1-zeta-class2

assert((b(matching(a1(),zeta(),2,verify_a())))==0);
// a1-zeta-class3

assert((b(matching(a1(),zeta(),3,verify_z())))==0);
// a1-alpha-class1

assert((b(matching(a1(),alpha(),1,alpha_class())))==0);
// a1-alpha-class1-exists

assert((b(has_row(a1(),alpha(),1)))==0);
// b1-active

assert((b((b1()).active))==1);
// b1-module-count

assert((module_count(b1()) as int)==2);
// b1-zeta-count

assert((count(b1(),zeta()) as int)==1);
// b1-alpha-count

assert((count(b1(),alpha()) as int)==1);
// b1-zeta-rows

assert((rows_from(b1(),zeta(),1).len() as int)==1);
// b1-zeta-class1

assert((b(matching(b1(),zeta(),1,verify_z())))==1);
// b1-zeta-class2

assert((b(matching(b1(),zeta(),2,verify_a())))==0);
// b1-zeta-class3

assert((b(matching(b1(),zeta(),3,verify_z())))==0);
// b1-alpha-class1

assert((b(matching(b1(),alpha(),1,alpha_class())))==1);
// b1-zeta-class1-exists

assert((b(has_row(b1(),zeta(),1)))==1);
// b1-alpha-class1-exists

assert((b(has_row(b1(),alpha(),1)))==1);
// a2-active

assert((b((a2()).active))==1);
// a2-module-count

assert((module_count(a2()) as int)==2);
// a2-zeta-count

assert((count(a2(),zeta()) as int)==2);
// a2-alpha-count

assert((count(a2(),alpha()) as int)==1);
// a2-zeta-rows

assert((rows_from(a2(),zeta(),1).len() as int)==2);
// a2-zeta-class1

assert((b(matching(a2(),zeta(),1,verify_z())))==1);
// a2-zeta-class2

assert((b(matching(a2(),zeta(),2,verify_a())))==1);
// a2-zeta-class3

assert((b(matching(a2(),zeta(),3,verify_z())))==0);
// a2-alpha-class1

assert((b(matching(a2(),alpha(),1,alpha_class())))==1);
// a2-zeta-class1-exists

assert((b(has_row(a2(),zeta(),1)))==1);
// a2-alpha-class1-exists

assert((b(has_row(a2(),alpha(),1)))==1);
// a3-active

assert((b((a3()).active))==1);
// a3-module-count

assert((module_count(a3()) as int)==2);
// a3-zeta-count

assert((count(a3(),zeta()) as int)==3);
// a3-alpha-count

assert((count(a3(),alpha()) as int)==1);
// a3-zeta-rows

assert((rows_from(a3(),zeta(),1).len() as int)==3);
// a3-zeta-class1

assert((b(matching(a3(),zeta(),1,verify_z())))==1);
// a3-zeta-class2

assert((b(matching(a3(),zeta(),2,verify_a())))==1);
// a3-zeta-class3

assert((b(matching(a3(),zeta(),3,verify_z())))==1);
// a3-alpha-class1

assert((b(matching(a3(),alpha(),1,alpha_class())))==1);
// a3-zeta-class1-exists

assert((b(has_row(a3(),zeta(),1)))==1);
// a3-alpha-class1-exists

assert((b(has_row(a3(),alpha(),1)))==1);
// after-report-active

assert((b((after_report()).active))==1);
// after-report-module-count

assert((module_count(after_report()) as int)==2);
// after-report-zeta-count

assert((count(after_report(),zeta()) as int)==3);
// after-report-alpha-count

assert((count(after_report(),alpha()) as int)==1);
// after-report-zeta-rows

assert((rows_from(after_report(),zeta(),1).len() as int)==3);
// after-report-zeta-class1

assert((b(matching(after_report(),zeta(),1,verify_z())))==1);
// after-report-zeta-class2

assert((b(matching(after_report(),zeta(),2,verify_a())))==1);
// after-report-zeta-class3

assert((b(matching(after_report(),zeta(),3,verify_z())))==1);
// after-report-alpha-class1

assert((b(matching(after_report(),alpha(),1,alpha_class())))==1);
// after-report-zeta-class1-exists

assert((b(has_row(after_report(),zeta(),1)))==1);
// after-report-alpha-class1-exists

assert((b(has_row(after_report(),alpha(),1)))==1);
// after-end-active

assert((b((after_end()).active))==0);
// after-end-module-count

assert((module_count(after_end()) as int)==0);
// after-end-zeta-count

assert((count(after_end(),zeta()) as int)==0);
// after-end-alpha-count

assert((count(after_end(),alpha()) as int)==0);
// after-end-zeta-rows

assert((rows_from(after_end(),zeta(),1).len() as int)==0);
// after-end-zeta-class1

assert((b(matching(after_end(),zeta(),1,verify_z())))==0);
// after-end-zeta-class2

assert((b(matching(after_end(),zeta(),2,verify_a())))==0);
// after-end-zeta-class3

assert((b(matching(after_end(),zeta(),3,verify_z())))==0);
// after-end-alpha-class1

assert((b(matching(after_end(),alpha(),1,alpha_class())))==0);
// after-end-zeta-class1-exists

assert((b(has_row(after_end(),zeta(),1)))==0);
// after-end-alpha-class1-exists

assert((b(has_row(after_end(),alpha(),1)))==0);
// after-inactive-active

assert((b((after_inactive()).active))==0);
// after-inactive-module-count

assert((module_count(after_inactive()) as int)==0);
// after-inactive-zeta-count

assert((count(after_inactive(),zeta()) as int)==0);
// after-inactive-alpha-count

assert((count(after_inactive(),alpha()) as int)==0);
// after-inactive-zeta-rows

assert((rows_from(after_inactive(),zeta(),1).len() as int)==0);
// after-inactive-zeta-class1

assert((b(matching(after_inactive(),zeta(),1,verify_z())))==0);
// after-inactive-zeta-class2

assert((b(matching(after_inactive(),zeta(),2,verify_a())))==0);
// after-inactive-zeta-class3

assert((b(matching(after_inactive(),zeta(),3,verify_z())))==0);
// after-inactive-alpha-class1

assert((b(matching(after_inactive(),alpha(),1,alpha_class())))==0);
// after-inactive-zeta-class1-exists

assert((b(has_row(after_inactive(),zeta(),1)))==0);
// after-inactive-alpha-class1-exists

assert((b(has_row(after_inactive(),alpha(),1)))==0);
// reset-empty-active

assert((b((reset_empty()).active))==1);
// reset-empty-module-count

assert((module_count(reset_empty()) as int)==0);
// reset-empty-zeta-count

assert((count(reset_empty(),zeta()) as int)==0);
// reset-empty-alpha-count

assert((count(reset_empty(),alpha()) as int)==0);
// reset-empty-zeta-rows

assert((rows_from(reset_empty(),zeta(),1).len() as int)==0);
// reset-empty-zeta-class1

assert((b(matching(reset_empty(),zeta(),1,verify_z())))==0);
// reset-empty-zeta-class2

assert((b(matching(reset_empty(),zeta(),2,verify_a())))==0);
// reset-empty-zeta-class3

assert((b(matching(reset_empty(),zeta(),3,verify_z())))==0);
// reset-empty-alpha-class1

assert((b(matching(reset_empty(),alpha(),1,alpha_class())))==0);
// reset-empty-zeta-class1-exists

assert((b(has_row(reset_empty(),zeta(),1)))==0);
// reset-empty-alpha-class1-exists

assert((b(has_row(reset_empty(),alpha(),1)))==0);
// reset-active

assert((b((reset()).active))==1);
// reset-module-count

assert((module_count(reset()) as int)==1);
// reset-zeta-count

assert((count(reset(),zeta()) as int)==1);
// reset-alpha-count

assert((count(reset(),alpha()) as int)==0);
// reset-zeta-rows

assert((rows_from(reset(),zeta(),1).len() as int)==1);
// reset-zeta-class1

assert((b(matching(reset(),zeta(),1,verify_z())))==1);
// reset-zeta-class2

assert((b(matching(reset(),zeta(),2,verify_a())))==0);
// reset-zeta-class3

assert((b(matching(reset(),zeta(),3,verify_z())))==0);
// reset-alpha-class1

assert((b(matching(reset(),alpha(),1,alpha_class())))==0);
// reset-zeta-class1-exists

assert((b(has_row(reset(),zeta(),1)))==1);
// reset-alpha-class1-exists

assert((b(has_row(reset(),alpha(),1)))==0);
// report-zeta-module-order

assert((b(forward()[0].module==zeta() && forward()[3].module==alpha()))==1);
// report-alpha-module-order

assert((b(reverse()[0].module==alpha() && reverse()[1].module==zeta()))==1);
// report-zeta-sequence-duplicates

assert((b(forward()[0].class==verify_z() && forward()[1].class==verify_a() && forward()[2].class==verify_z()))==1);
// cli-single-skipped

assert((b(verbose_skip_count(cli_resource(),seq![cli_class()],true)>0))==1);
// cli-all-reported-class

assert((b(cli_all_report()==seq![Row{module:cli_module(),sequence:1,class:cli_class()}]))==1);
}}
fn main(){}
