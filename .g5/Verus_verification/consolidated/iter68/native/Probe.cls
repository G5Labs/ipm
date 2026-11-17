Include %IPM.Common

Class Iter68.Probe
{

ClassMethod Observe(key As %String, value As %Integer)
{
    write !,"OBS|",key,"|",value,!
}

ClassMethod RunCommand(key As %String, command As %String) As %Status
{
    set sc = ##class(%IPM.Main).Shell(command)
    do ..Observe(key, $$$ISOK(sc))
    if $$$ISERR(sc) { write !,"ERROR|",key,"|",$system.Status.GetErrorText(sc),! }
    quit sc
}

ClassMethod Revision() As %Integer
{
    quit +$classmethod("G5Admission.Sample", "Revision")
}

ClassMethod Root() As %String
{
    set module = ##class(%IPM.Storage.Module).NameOpen("g5-admission")
    quit module.Root
}

ClassMethod RememberRoot()
{
    do ..Observe("callable-before", ..Revision())
    set file = ..Root() _ "g5-iter68-root-sentinel"
    set stream = ##class(%Stream.FileCharacter).%New()
    set stream.Filename = file
    $$$ThrowOnError(stream.Write("existing install"))
    $$$ThrowOnError(stream.%Save())
}

ClassMethod ReadMetadata()
{
    set ref = ##class(%IPM.Storage.QualifiedModuleInfo).%New()
    set ref.Name = "g5-admission", ref.VersionString = "1.0.0", ref.ServerName = "boundary"
    $$$ThrowOnError(##class(%IPM.Repo.Utils).RetrieveModuleManifest(ref, .stream))
    set module = ##class(%IPM.Utils.Module).GetModuleObjectFromStream(stream, .found)
    do ..Observe("metadata-is-revision-1", found && (module.Description = "revision-1"))
}

ClassMethod AfterRefusal()
{
    do ..Observe("callable-after-refusal", ..Revision())
    do ..Observe("root-preserved", ##class(%File).Exists(..Root() _ "g5-iter68-root-sentinel"))
    set module = ##class(%IPM.Storage.Module).NameOpen("g5-admission")
    do ..Observe("refusal-preserves-module", module.Description = "revision-1")
    set rows = ##class(%SQL.Statement).%ExecDirect(, "SELECT TOP 1 ID FROM %IPM_General.History WHERE Package = 'g5-admission' AND Action = 'install' ORDER BY ID DESC")
    if rows.%Next() {
        set history = ##class(%IPM.General.History).%OpenId(rows.%Get("ID"))
        do ..Observe("failed-history", $$$ISERR(history.Success) && 'history.Committed && history.Finalized)
    }
}

}
