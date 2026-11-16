/// Calls the private SyncCompile step of the real sync pipeline for one installed module.
Class Iter4.SyncProbe Extends %IPM.General.Sync.Pipeline
{

ClassMethod Compile(moduleName As %String) As %Status
{
    set module = ##class(%IPM.Storage.Module).NameOpen(moduleName, , .sc)
    if $$$ISERR(sc) {
        quit sc
    }
    quit ..SyncCompile(module, module.GetOrderedResourceList(), 0)
}

/// Change a class parameter's definition without compiling it.
ClassMethod Stale(className As %String, value As %String) As %Status
{
    set param = ##class(%Dictionary.ParameterDefinition).%OpenId(className_"||V", , .sc)
    if $$$ISERR(sc) {
        quit sc
    }
    set param.Default = value
    quit param.%Save()
}

ClassMethod Case(case As %String, moduleName As %String, active As %Boolean, classes As %List)
{
    kill ^||IPM.Sync.Summary
    if active {
        set ^||IPM.Sync.Summary("Active") = 1
    }
    set ptr = 0
    while $listnext(classes, ptr, cls) {
        do ##class(Iter4.Probe).Obs(case, cls_"_stale_saved", $system.Status.IsOK(..Stale(cls, 2)))
    }
    set ^G5I4FailResolve = 1
    set sc = ..Compile(moduleName)
    kill ^G5I4FailResolve
    do ##class(Iter4.Probe).Obs(case, "ok", $system.Status.IsOK(sc))
    set ptr = 0
    while $listnext(classes, ptr, cls) {
        do ##class(Iter4.Probe).Obs(case, cls_"_compiled_value", $parameter(cls, "V"))
    }
    do ##class(Iter4.Probe).Obs(case, "retained_warnings", $get(^||IPM.Sync.Summary("Module", moduleName, "WarningCount"), 0))
    kill ^||IPM.Sync.Summary
}

ClassMethod Drive()
{
    do ##class(Iter4.Probe).Cmd("Y0-load-a", "load /tmp/g5-iter4/synca")
    do ##class(Iter4.Probe).Cmd("Y0-load-b", "load /tmp/g5-iter4/syncb")
    // A: one resource cannot be resolved, a second supplies a defined class.
    do ..Case("Y1-survivor", "g5i4-sync-a", 1, $listbuild("G5I4SyncA.Good.Dep", "G5I4SyncA.Bad.Item"))
    // B: the only compilable resource cannot be resolved and no summary is active.
    do ..Case("Y2-none-left", "g5i4-sync-b", 0, $listbuild("G5I4SyncB.Bad.Item"))
}

}
