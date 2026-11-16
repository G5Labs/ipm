Include %occErrors

Class G5FlagOverride20261114.Probe
{

ClassMethod Child(case As %String) As %Status
{
    set journal = '($zboolean($zjob, 1024, 1) = 1024)
    write !,"OBS ",case," child.tlevel=",$tlevel," child.journal=",journal,!
    set ^G5FlagOverride20261114(case, "child-write") = "written"
    quit $$$OK
}

ClassMethod Parent(case As %String, fail As %Boolean) As %Status
{
    set journal = '($zboolean($zjob, 1024, 1) = 1024)
    write !,"OBS ",case," parent.tlevel=",$tlevel," parent.journal=",journal,!
    set ^G5FlagOverride20261114(case, "parent-write") = "written"
    if fail { quit $$$ERROR($$$GeneralError, "G5 flag override: deliberate parent Activate failure") }
    quit $$$OK
}

ClassMethod Run(case As %String, path As %String, flag As %String, zero As %Boolean, root As %String, child As %String)
{
    write !,"OBS ",case," entry.tlevel=",$tlevel," entry.journal=",'($zboolean($zjob, 1024, 1) = 1024),!
    write "OBS ",case," before.root=",##class(%IPM.Storage.Module).NameExists(root)," before.child=",##class(%IPM.Storage.Module).NameExists(child),!
    set cmd = "load " _ path _ $select(zero: " -D" _ flag _ "=0", 1: "")
    write "CMD ",case," ",cmd,!
    set sc = ##class(%IPM.Main).Shell(cmd)
    write !,"OBS ",case," status=",$select($system.Status.IsOK(sc): "OK",1:"ERR"),!
    if $system.Status.IsError(sc) { write "OBS ",case," error=",$system.Status.GetErrorText(sc),! }
    write "OBS ",case," exit.tlevel=",$tlevel," exit.journal=",'($zboolean($zjob, 1024, 1) = 1024),!
    write "OBS ",case," stored.root=",##class(%IPM.Storage.Module).NameExists(root)," stored.child=",##class(%IPM.Storage.Module).NameExists(child),!
    write "OBS ",case," stored.childwrite=",$data(^G5FlagOverride20261114(case, "child-write"))," stored.parentwrite=",$data(^G5FlagOverride20261114(case, "parent-write")),!
}

ClassMethod Snapshot(label As %String)
{
    set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Name,VersionString FROM %IPM_Storage.ModuleItem ORDER BY ID")
    while rs.%Next() { write "SNAP ",label," module=",rs.ID,"|",rs.Name,"|",rs.VersionString,! }
    set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,Name,Enabled FROM %IPM_Repo.Definition ORDER BY ID")
    while rs.%Next() { write "SNAP ",label," repo=",rs.ID,"|",rs.Name,"|",rs.Enabled,! }
}

ClassMethod Cleanup(root As %String, child As %String)
{
    for name = root,child {
        if ##class(%IPM.Storage.Module).NameExists(name) {
            set sc=##class(%IPM.Main).Shell("uninstall "_name)
            write "CLEAN ",name," status=",$system.Status.IsOK(sc),!
        }
    }
}

}
