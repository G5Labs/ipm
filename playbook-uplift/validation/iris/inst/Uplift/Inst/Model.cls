/// Probes for facts the update-step model (models/verus/update_steps.rs) depends on that the native checks did not
/// cover. Prints OBS|nc-inst-model-...| lines (Uplift.Probe.Env:Emit via Uplift.Inst.Checks:Obs).
Class Uplift.Inst.Model
{

Parameter FIX = "/tmp/uplift/inst/fixtures/order/";

ClassMethod Cleanup()
{
    do ##class(Uplift.Inst.Checks).Reset()
    do $system.OBJ.DeletePackage("NcInstOrd", "-d")
    kill ^NcInstOrd
}

ClassMethod StepRows(ByRef kv)
{
    set n = 0
    for t = "AnyMember", "PrimaryOnly" {
        set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ClassName, MethodName, TimeStampStart, TimeStampEnd, Status FROM %IPM_Storage_UpdateStep."_t_" WHERE ClassName %STARTSWITH ?", "NcInstOrd.Upd.")
        while rs.%Next() {
            set n = n + 1
            set cls = $piece(rs.%Get("ClassName"), ".", *)
            set kv($select(t = "AnyMember": "any:", 1: "pri:")_cls_"."_rs.%Get("MethodName")) = "start:"_(rs.%Get("TimeStampStart") '= "")_"/end:"_(rs.%Get("TimeStampEnd") '= "")_"/ok:"_$system.Status.IsOK(rs.%Get("Status"))
        }
    }
    set kv("stepRows") = n
}

/// Walk order of update classes (V10 after V9?), and package classes whose name does not end in a distinct number.
ClassMethod Order()
{
    set C = "Uplift.Inst.Checks"
    do ..Cleanup()
    do $classmethod(C, "Put", .kv, "load100", $classmethod(C, "Run", "load "_..#FIX_"1.0.0/"))
    set kv("orderAfterLoad") = $get(^NcInstOrd("order"), "<none>")
    do $classmethod(C, "Put", .kv, "update200", $classmethod(C, "Run", "update nc-inst-ord -path "_..#FIX_"2.0.0/"))
    set kv("order") = $get(^NcInstOrd("order"), "<none>")
    set kv("version") = $classmethod(C, "Ver", "nc-inst-ord")
    do ..StepRows(.kv)
    do $classmethod(C, "Obs", "nc-inst-model-class-order", .kv)

    // V3 and a helper class V3Helper both strip to class number 3
    kill kv
    kill ^NcInstOrd("order")
    do $classmethod(C, "Put", .kv, "update300", $classmethod(C, "Run", "update nc-inst-ord -path "_..#FIX_"3.0.0/"))
    set kv("order") = $get(^NcInstOrd("order"), "<none>")
    set kv("version") = $classmethod(C, "Ver", "nc-inst-ord")
    do ..StepRows(.kv)
    do $classmethod(C, "Obs", "nc-inst-model-class-number-collision", .kv)

    // a helper class whose name has no digit (Validator) strips to an empty class number
    kill kv
    kill ^NcInstOrd("order")
    do $classmethod(C, "Put", .kv, "update400", $classmethod(C, "Run", "update nc-inst-ord -path "_..#FIX_"4.0.0/"))
    set kv("order") = $get(^NcInstOrd("order"), "<none>")
    set kv("version") = $classmethod(C, "Ver", "nc-inst-ord")
    do ..StepRows(.kv)
    do $classmethod(C, "Obs", "nc-inst-model-class-without-number", .kv)
}

}
