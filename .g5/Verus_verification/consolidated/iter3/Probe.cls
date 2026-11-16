Include %IPM.Common
Class Consolidated.Iter3.Probe
{
Parameter I3Deploy = 0;
ClassMethod Obs(key, value)
{
    write !,"OBS|",key,"|",value,!
}
ClassMethod Failed() As %Status
{
    quit $$$ERROR($$$GeneralError,"controlled Invoke failure")
}
ClassMethod ArtifactSelection()
{
    set cls="%IPM.ResourceProcessor.ArtifactoryTarball"
    for raw="{""name"":{""$match"":""a*""},""repo"":""r"",""@branch"":""main""}","{""repo"":""r""}","{""name"":""a"",""repo"":{""$match"":""r*""}}", "{""name"":""a"",""size"":3}" {
        try {set c=$classmethod(cls,"ParseAnonymousFilter",raw) do ..Obs("filter-"_$increment(n),1)}
        catch ex {do ..Obs("filter-"_$increment(n),0)}
    }
    set c=$classmethod(cls,"ParseAnonymousFilter","{""name"":{""$match"":""a*""},""repo"":""r"",""@branch"":""main""}")
    set rows=[{"repo":"r","path":"/a1","lastUpdated":"2","properties":{"branch":["main"]}},{"repo":"bad","path":"/a2","lastUpdated":"9","properties":{"branch":["main"]}},{"repo":"r","path":"/a3","lastUpdated":"3","properties":{"branch":["dev","main"]}},{"repo":"r","path":"/a4","lastUpdated":"8","properties":{"branch":["dev"]}}]
    do ..Obs("selection",$classmethod(cls,"SelectNewestArtifact",rows,c))
    set rows=[{"repo":"r","path":"/a1","lastUpdated":"3","properties":{"branch":["main"]}},{"repo":"r","path":"/a2","lastUpdated":"3","properties":{"branch":["main"]}}]
    do ..Obs("tie",$classmethod(cls,"SelectNewestArtifact",rows,c))
    for pair="a.b|a.b","axb|a.b","ab|a?","xab|a*" {
        do ..Obs("wild-"_$increment(w),$classmethod(cls,"WildcardMatch",$piece(pair,"|",1),$piece(pair,"|",2)))
    }
}
ClassMethod ArtifactHTTP()
{
    set cls="%IPM.ResourceProcessor.ArtifactoryTarball"
    try {
        set path=$classmethod(cls,"GetArtifactPath","localhost",18433,"/aql-secure","u","p","fixture-token","{}")
        do ..Obs("metadata-path",path)
        try {set file=$classmethod(cls,"RetrieveArtifact","localhost",18433,path,"/tmp/g5-iter3/downloads/") do ..Obs("secure-bytes",1)}
        catch ex {do ..Obs("secure-bytes",0) do ..Obs("secure-error",$system.Status.GetErrorText(ex.AsStatus()))}
    } catch ex {do ..Obs("metadata-error",ex.DisplayString())}
    for kind="first","second" {
        set path=$classmethod(cls,"GetArtifactPath","localhost",18433,"/aql-"_kind,"u","p","fixture-token","{}")
        do ..Obs("aql-"_kind,path)
    }
    set file=$classmethod(cls,"RetrieveArtifact","localhost",18433,"artifactory/public/./payload.tgz","/tmp/g5-iter3/cache/")
    set s=##class(%Stream.FileBinary).%New() do s.LinkToFile("/tmp/g5-iter3/cache/"_file)
    do ..Obs("cached-bytes",s.Read())
}
ClassMethod ArtifactLifecycle()
{
    set m=##class(%IPM.Storage.Module).%New(),m.Name="g5-iter3-artifact",m.Root="/tmp/g5-iter3/module/",m.DeveloperMode=1
    set r=##class(%IPM.Storage.ResourceReference).%New(),r.Module=m,r.Name="/assets"
    do r.Attributes.SetAt("/tmp/g5-iter3/activated/","InstallDirectory")
    set p=##class(%IPM.ResourceProcessor.ArtifactoryTarball).%New(r)
    set params("DeveloperMode")=1
    set sc=p.OnAfterPhase("Activate",.params)
    do ..Obs("activate-local",$system.Status.IsOK(sc))
    do ..Obs("activate-old",##class(%File).Exists("/tmp/g5-iter3/activated/old.txt"))
    do ..Obs("activate-new",##class(%File).Exists("/tmp/g5-iter3/activated/new.txt"))
    set params("ModuleExportPath")="/tmp/g5-iter3/export/assets/"
    set sc=p.OnExportItem("ignored","ignored",.ip,.params,.handled)
    do ..Obs("export-local",$system.Status.IsOK(sc))
    do ..Obs("export-handled",handled)
    do ..Obs("export-old",##class(%File).Exists("/tmp/g5-iter3/export/assets/old.txt"))
    do ..Obs("export-sibling",##class(%File).Exists("/tmp/g5-iter3/export/sibling.txt"))
}
ClassMethod Defaults()
{
    set m=##class(%IPM.Storage.Module).%New()
    set d=##class(%IPM.Storage.ModuleSetting.ProcessorDefault).%New(),d.Class="Consolidated.Iter3.Defaults"
    do d.Attributes.SetAt("1","Mode") do m.Defaults.Insert(d)
    set d=##class(%IPM.Storage.ModuleSetting.ProcessorDefault).%New(),d.Class="Consolidated.Iter3.Defaults"
    set c=##class(%IPM.Storage.ModuleSetting.ProcessorDefault.Condition).%New(),c.Attribute="Mode",c.Value="1"
    do d.Conditions.Insert(c)
    do d.Attributes.SetAt("2","Mode") do d.Attributes.SetAt("3","Other") do m.Defaults.Insert(d)
    set r=##class(%IPM.Storage.ResourceReference).%New(),r.Module=m,r.Name="default"
    set p=##class(Consolidated.Iter3.Defaults).%New(r)
    do ..Obs("default-mode",p.Mode) do ..Obs("default-other",p.Other)
    do r.Attributes.SetAt("","Mode")
    set p=##class(Consolidated.Iter3.Defaults).%New(r)
    do ..Obs("default-explicit-empty",p.Mode="")
    do ..Obs("default-unmatched-other",p.Other)
}
ClassMethod Requirements()
{
    set ^||I3Health=1,^||I3Interop=1
    set r=##class(Consolidated.Iter3.Requirements).%New()
    do ..Obs("health-unset",$system.Status.IsOK(r.CheckHealth()))
    set r.Health=0 do ..Obs("health-false",$system.Status.IsOK(r.CheckHealth()))
    set r.Health=1,r.Interoperability="disabled",^||I3Interop=0
    do ..Obs("health-implies-interop",$system.Status.IsOK(r.CheckInteroperability()))
    set r.Health="",^||I3Interop=1
    do ..Obs("disabled-allows-enabled",$system.Status.IsOK(r.CheckInteroperability()))
    set r.SYSNamespace=1
    do ..Obs("requires-sys",$system.Status.IsOK(r.CheckSYSNamespace()))
    new $namespace set $namespace="%SYS"
    do ..Obs("requires-sys-in-sys",$system.Status.IsOK(r.CheckSYSNamespace()))
}
ClassMethod Public()
{
    set m=##class(%IPM.Storage.Module).%New()
    set r=##class(%IPM.Storage.InvokeReference).%New(),r.Module=m,r.Class="Consolidated.Iter3.Probe",r.Method="Failed"
    set sc=r.doInvoke(.params) do ..Obs("invoke-default",$system.Status.IsOK(sc))
    set r.CheckStatus=0,sc=r.doInvoke(.params) do ..Obs("invoke-unchecked",$system.Status.IsOK(sc))
    set r.CheckStatus=1,sc=r.doInvoke(.params) do ..Obs("invoke-checked",$system.Status.IsOK(sc))
    set key="g5-iter3-setting"
    do ##class(%IPM.Repo.UniversalSettings).SetValue(key,"first")
    do ##class(%IPM.Repo.UniversalSettings).SetValue(key,"second",0)
    do ..Obs("setting-no-overwrite",##class(%IPM.Repo.UniversalSettings).GetValue(key))
    do ##class(%IPM.Repo.UniversalSettings).SetValue(key,"",1)
    do ##class(%IPM.Repo.UniversalSettings).SetValue(key,"third",0)
    do ..Obs("setting-empty-allows-write",##class(%IPM.Repo.UniversalSettings).GetValue(key))
    do ..Obs("setting-first",##class(%IPM.Repo.UniversalSettings).GetDefaultValue(key))
    set old=##class(%IPM.Repo.UniversalSettings).GetTestReportFormat()
    set sc=##class(%IPM.Repo.UniversalSettings).SetTestReportFormat("JsOn")
    do ..Obs("format-case",##class(%IPM.Repo.UniversalSettings).GetTestReportFormat())
    set sc=##class(%IPM.Repo.UniversalSettings).SetTestReportFormat("invalid")
    do ..Obs("format-invalid",$system.Status.IsOK(sc))
    do ..Obs("format-retained",##class(%IPM.Repo.UniversalSettings).GetTestReportFormat())
    do ##class(%IPM.Repo.UniversalSettings).SetTestReportFormat(old)
    set ref=##class(%IPM.Storage.ResourceReference).%New(),ref.Module=m,ref.Name="tests"
    do ref.Attributes.SetAt("A","Package")
    set p=##class(%IPM.ResourceProcessor.Test).%New(ref),p.Package="A"
    set params("UnitTest","Case")="AB"
    do ..Obs("test-prefix",p.TestsShouldRun("tEsT",.params))
    do ..Obs("owns-sibling",p.OwnsClassName("AB"))
    do ..Obs("owns-child",p.OwnsClassName("A.C"))
    set params("UnitTest","Case")="Z",params("UnitTest","Suite")=""
    do ..Obs("suite-empty-shadows",p.TestsShouldRun("Test",.params))
    kill params("UnitTest","Suite")
    do ..Obs("case-alone",p.TestsShouldRun("Test",.params))
    set p.Package="",p.Class="X"
    do ..Obs("class-ignores-case",p.TestsShouldRun("Test",.params))
}
ClassMethod ExportFailure()
{
    set m=##class(%IPM.Storage.Module).%New(),m.Name="g5-iter3-doc",m.Root="/tmp/g5-iter3/module/"
    set r=##class(%IPM.Storage.ResourceReference).%New(),r.Module=m,r.Name="Consolidated.Iter3.Probe.CLS"
    set p=##class(Consolidated.Iter3.FailDocument).%New(r),p.Format="UDL"
    set sc=p.OnExportDeployedItem("","",.ip,.params,.handled)
    do ..Obs("deployed-hook-error",$system.Status.IsOK(sc))
    set sc=p.OnExportItem("/tmp/g5-iter3/doc/Probe.cls","Consolidated.Iter3.Probe.cls",.ip,.params,.handled)
    do ..Obs("export-overwrites-error",$system.Status.IsOK(sc))
    do ..Obs("export-source-file",##class(%File).Exists("/tmp/g5-iter3/doc/Probe.cls"))
}
ClassMethod DeploymentAndSearch()
{
    set m=##class(%IPM.Storage.Module).%New(),m.Name="g5-iter3-deploy"
    set lc=##class(%IPM.Lifecycle.Module).%New(m)
    set params("Deploy","Parameter")="I3Deploy",info("Name")="Consolidated.Iter3.Probe.CLS",info("Deploy")=1
    set sc=lc.OnDetermineResourceDeployability(.params,.info,.d)
    do ..Obs("deploy-parameter-false",d)
    kill params("Deploy")
    set sc=lc.OnDetermineResourceDeployability(.params,.info,.d)
    do ..Obs("deploy-resource-true",d)
    set info("Deploy")="",sc=lc.OnDetermineResourceDeployability(.params,.info,.d)
    do ..Obs("deploy-module-default",d)
    set lc=##class(%IPM.Lifecycle.DeployedModule).%New(m),sc=lc.OnDetermineResourceDeployability(.params,.info,.d)
    do ..Obs("deploy-deployedmodule-default",d)
    set repo=##class(%IPM.Repo.Filesystem.Definition).ServerDefinitionKeyOpen("g5-iter3-search")
    if '$isobject(repo) {set repo=##class(%IPM.Repo.Filesystem.Definition).%New()}
    set repo.Name="g5-iter3-search",repo.Root="/tmp/g5-iter3/repo/",repo.Enabled=1,sc=repo.%Save()
    do ..Obs("fs-save",$system.Status.IsOK(sc))
    set sc=repo.BuildCache() do ..Obs("fs-cache",$system.Status.IsOK(sc))
    set criteria=##class(%IPM.Repo.SearchCriteria).%New(),criteria.Name="g5-iter3-reposearch",criteria.Registry=repo.Name
    set manager=##class(%IPM.Repo.Manager).%Get(),sc=manager.SearchRepositoriesForModule(criteria,.results)
    do ..Obs("fs-name-count",results.Count())
    do criteria.Keywords.Insert("probe")
    set sc=manager.SearchRepositoriesForModule(criteria,.results)
    do ..Obs("fs-keyword-count",results.Count())
}
ClassMethod ProjectReaders()
{
    set project=##class(%Studio.Project).%New(),project.Name="g5-iter3-empty"
    do ..Obs("project-empty-check",$system.Status.IsOK(project.Check()))
    do project.AddItem("Consolidated.Iter3.Missing.CLS")
    do ..Obs("project-before-check",project.Items.Count())
    do ..Obs("project-invalid-check",$system.Status.IsOK(project.Check()))
    do ..Obs("project-after-check",project.Items.Count())
    set m=##class(%IPM.Storage.Module).NameOpen("g5-iter3-project")
    if '$isobject(m) {set m=##class(%IPM.Storage.Module).%New()}
    set m.Name="g5-iter3-project",m.VersionString="1.0.0",m.Packaging="module",m.Root="/tmp/g5-iter3/project/",m.Deployed=0,sc=m.%Save()
    do ..Obs("project-module-save",$system.Status.IsOK(sc))
    set path=##class(%IPM.Utils.Module).GetDeployedProjectPath(m.Root)
    do ##class(%IPM.Utils.File).CreateDirectoryChain(##class(%File).GetDirectory(path))
    set stream=##class(%Stream.FileCharacter).%New(),stream.Filename=path
    do stream.Write("invalid deployed project") do stream.%Save()
    if m.Resources.Count()>0 {set r=m.Resources.GetAt(1)} else {set r=##class(%IPM.Storage.ResourceReference).%New(),r.Module=m}
    set r.Name="Consolidated.Iter3.Probe.CLS",r.Deploy=1
    set p=##class(%IPM.ResourceProcessor.Default.Document).%New(r),params("NoMapping")=1
    set sc=p.OnPhase("Reload",.params,.handled)
    do ..Obs("project-install-error",$system.Status.IsOK(sc))
    do ..Obs("project-failed-in-installer",$system.Status.GetErrorText(sc)["Failed to install deployed code")
    do ..Obs("project-deployed-on-error",m.Deployed)
    set stored=##class(%SQL.Statement).%ExecDirect(,"select Deployed from %IPM_Storage.ModuleItem where Name = ?",m.Name)
    if stored.%Next() {do ..Obs("project-deployed-stored-on-error",stored.%Get("Deployed"))}
    do ..Obs("project-marker-on-error",$data($$$DeployedProjectInstalled(path)))
    set m.Deployed=0,sc=m.%Save()
    set duplicate=##class(%IPM.Storage.ResourceReference).%New(),duplicate.Module=m,duplicate.Name=r.Name,duplicate.Deploy=1
    set p2=##class(%IPM.ResourceProcessor.Default.Document).%New(duplicate)
    set sc=p2.OnPhase("Reload",.params,.handled)
    do ..Obs("project-duplicate-save-error",$system.Status.GetErrorText(sc)["already defined")
    do ..Obs("project-duplicate-object",m.Deployed)
    set stored=##class(%SQL.Statement).%ExecDirect(,"select Deployed from %IPM_Storage.ModuleItem where Name = ?",m.Name)
    if stored.%Next() {do ..Obs("project-duplicate-store",stored.%Get("Deployed"))}

}

ClassMethod ArtifactBranch(label, developer)
{
    set m=##class(%IPM.Storage.Module).%New(),m.Name="g5-iter3-branch",m.Root="/tmp/g5-iter3/module/",m.DeveloperMode=developer
    set r=##class(%IPM.Storage.ResourceReference).%New(),r.Module=m,r.Name="/"_label
    do r.Attributes.SetAt("/tmp/g5-iter3/branch-target/"_label_"/","InstallDirectory")
    do r.Attributes.SetAt("{""name"":""corrupt.tgz""}","Filter")
    set p=##class(%IPM.ResourceProcessor.ArtifactoryTarball).%New(r)
    set params("DeveloperMode")=developer
    set sc=p.OnAfterPhase("Activate",.params)
    do ..Obs("branch-"_label,$system.Status.IsOK(sc))
    do ..Obs("branch-dir-"_label,##class(%File).DirectoryExists(m.Root_r.Name))
}

ClassMethod ArchiveBoundary()
{
    set s=##class(%Stream.FileBinaryGzip).%New(),s.Filename="/tmp/g5-iter3/not-gzip.tgz"
    set data=s.Read(512,.sc)
    do ..Obs("gzip-bad-read-length",$length(data)) do ..Obs("gzip-bad-read-status",$system.Status.IsOK(sc))
    set sc=##class(%IPM.General.Archive).Extract("/tmp/g5-iter3/not-gzip.tgz","/tmp/g5-iter3/not-gzip-out/")
    do ..Obs("archive-bad-gzip",$system.Status.IsOK(sc))
    set sc=##class(%IPM.General.Archive).Extract("/tmp/g5-iter3/bad-checksum.tgz","/tmp/g5-iter3/bad-checksum-out/")
    do ..Obs("archive-bad-checksum",$system.Status.IsOK(sc))
}

}
