Class Iter27Hook.Installer
{
ClassMethod OnInstallComponent(database,verbose,developer,isUpgrade) { quit 1 }
ClassMethod OnConfigureComponent(namespace,verbose,ByRef vars) { quit 1 }
ClassMethod OnUnconfigureComponent(namespace,verbose,purge,ByRef vars)
{
 set ^||Iter27Hook("called")=$increment(^||Iter27Hook("called"))
 set ^||Iter27Hook("purge")=purge
 set ^||Iter27Hook("top")=+$get(vars("Purge"))
 set ^||Iter27Hook("nested")=+$get(vars("Configure","Purge"))
 if purge { quit ##class(Iter27Hook.Row).%DeleteExtent() }
 quit 1
}
ClassMethod OnUnInstallComponent(database,verbose)
{
 set ^||Iter27Hook("after")=$increment(^||Iter27Hook("after"))
 quit 1
}
}
