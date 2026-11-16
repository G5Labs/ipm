Include %IPM.Common
Class Iter21.Temporal
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(label,command)
{
 write !,"BEGIN|",label,!
 set sc=##class(%IPM.Main).Shell(command)
 write !,"END|",label,!
 do ..Obs(label,$$$ISOK(sc))
 if $$$ISERR(sc) {write !,"STATUS|",label,"|",$system.Status.GetErrorText(sc),!}
}
ClassMethod Text(s)
{
 do s.Rewind() set text=""
 while 's.AtEnd {set text=text_s.Read(30000)}
 return text
}
ClassMethod Marker(text) { return $select(text["source-A":1,text["source-B":2,1:0) }
ClassMethod Version(text) { return $select(text["<Version>1.0.0</Version>":1,text["<Version>2.0.0</Version>":2,1:0) }
ClassMethod Physical(label)
{
 set f=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(f.LinkToFile("/tmp/iter21/repo/active/module.xml"))
 set text=..Text(f)
 do ..Obs(label_"-physical-version",..Version(text)),..Obs(label_"-physical-marker",..Marker(text))
}
ClassMethod Cache(label,version)
{
 set d=##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("temporal")
 do ..Obs(label_"-definition",$isobject(d)),..Obs(label_"-repo-root",d.Root="/tmp/iter21/repo/")
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT ID,VersionString,SubDirectory FROM %IPM_Repo_Filesystem.Cache WHERE Root=? AND Name=? ORDER BY VersionString","/tmp/iter21/repo/","iter21-cache")
 $$$ThrowSQLIfError(rs.%SQLCODE,rs.%Message)
 set count=0,cacheText=""
 while rs.%Next() {
  set count=count+1,row=##class(%IPM.Repo.Filesystem.Cache).%OpenId(rs.%Get("ID"))
  set cacheText=..Text(row.Manifest)
  do ..Obs(label_"-cache-version",$select(row.VersionString="1.0.0":1,row.VersionString="2.0.0":2,1:0))
  do ..Obs(label_"-cache-manifest-version",..Version(cacheText)),..Obs(label_"-cache-marker",..Marker(cacheText)),..Obs(label_"-cache-directory",row.SubDirectory="active")
  write !,"CACHE|",label,"|",rs.%Get("ID"),"|",row.VersionString,"|",row.SubDirectory,!
 }
 do ..Obs(label_"-cache-count",count)
 set service=d.GetPackageService(),ref=##class(%IPM.Storage.ModuleInfo).%New()
 set ref.Name="iter21-cache",ref.VersionString=version
 set text=..Text(service.GetModuleManifest(ref)),directory=service.GetModuleDirectory(ref)
 do ..Obs(label_"-service-version",..Version(text)),..Obs(label_"-service-marker",..Marker(text)),..Obs(label_"-service-cache-equal",text=cacheText),..Obs(label_"-service-directory",directory="/tmp/iter21/repo/active/")
 do ..Physical(label)
 do ..Cmd("search-"_label,"search iter21-cache -versions")
}
ClassMethod Current(label,requested="")
{
 set m=##class(%IPM.Storage.Module).NameOpen("iter21-cache")
 do ..Obs(label_"-installed",$isobject(m))
 if $isobject(m) {
  if requested'= "" {do ..Obs(label_"-requested-version-preserved",m.VersionString=requested)}
  do ..Obs(label_"-installed-version",$select(m.VersionString="1.0.0":1,m.VersionString="2.0.0":2,1:0)),..Obs(label_"-installed-marker",..Marker(m.Description)),..Obs(label_"-installed-description-exact",m.Description=$select(m.VersionString="1.0.0":"source-A",1:"source-B"))
  do ..Obs(label_"-installed-root",##class(%File).NormalizeDirectory(m.Root)="/tmp/iter21/repo/active/"),..Obs(label_"-installed-supplier",m.Repository="temporal")
 }
 set available=$system.CLS.IsMthd("Iter21Cache.Payload","Who")>0
 do ..Obs(label_"-code",available)
 if available {do ..Obs(label_"-payload",$classmethod("Iter21Cache.Payload","Who"))}
 do ..Cmd("show-"_label,"show iter21-cache")
}
ClassMethod Run()
{
 do ..Cmd("repo-clear","repo -delete-all")
 do ..Cmd("repo-create","repo -fs -name temporal -depth 1 -path /tmp/iter21/repo")
 do ..Cache("initial","1.0.0")
 do ..Cmd("install-stable","install temporal/iter21-cache 1.0.0"),..Current("stable","1.0.0")
 do ..Obs("replace-manifest",##class(%File).CopyFile("/tmp/iter21/replacement/module.xml","/tmp/iter21/repo/active/module.xml"))
 do ..Obs("replace-code",##class(%File).CopyFile("/tmp/iter21/replacement/src/Iter21Cache/Payload.cls","/tmp/iter21/repo/active/src/Iter21Cache/Payload.cls"))
 do ..Cache("stale","1.0.0"),..Current("edited")
 do ..Cmd("install-stale","install temporal/iter21-cache 1.0.0"),..Current("stale","1.0.0")
 do ..Cmd("repo-reconfigure","repo -fs -name temporal -depth 1 -path /tmp/iter21/repo")
 do ..Cache("refreshed","2.0.0")
 do ..Cmd("install-old-after-refresh","install temporal/iter21-cache 1.0.0"),..Current("refused")
 do ..Cmd("install-new-after-refresh","install temporal/iter21-cache 2.0.0"),..Current("new","2.0.0")
 do ..Obs("final-tlevel",$tlevel)
}
}
