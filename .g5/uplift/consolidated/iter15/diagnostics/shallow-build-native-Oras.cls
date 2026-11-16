Include %IPM.Common
Class Iter15.Oras
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) {set ^Iter15Error(k)=$system.Status.GetErrorText(sc) write !,"RAW|",k,"|",^Iter15Error(k),!}
 return $$$ISOK(sc)
}
ClassMethod Version()
{
 set m=##class(%IPM.Storage.Module).NameOpen("platform-filter-test")
 if '$isobject(m) {return 0}
 return +m.VersionString
}
ClassMethod PublicVersion()
{
 if '$system.CLS.IsMthd("PlatformFilterTest.Public","Version") {return 0}
 return +$classmethod("PlatformFilterTest.Public","Version")
}
ClassMethod Package(folder,path,label="")
{
 if label="" {set label=folder}
 do ..Cmd("load-"_label,"load -nodev /tmp/iter15/modules/test/"_folder)
 do ..Cmd("package-"_label,"package platform-filter-test -only -path "_path)
}
ClassMethod PublishPlatform(label,path,platform)
{
 set m=##class(%IPM.Storage.Module).NameOpen("platform-filter-test",,.sc)
 $$$ThrowOnError(sc)
 set d=##class(%IPM.Repo.Oras.Definition).%New(),d.URL="http://oras:5000",svc=d.GetPublishService()
 set info=##class(%IPM.Repo.Remote.ModuleInfo).%New(),info.Name=m.Name,info.VersionString=m.VersionString,info.Deployed=1,info.PlatformVersion=platform
 set f=##class(%Stream.FileBinary).%New(),f.Filename=path_".tgz"
 $$$ThrowOnError(info.Payload.CopyFrom(f))
 set manifest=##class(%Stream.GlobalCharacter).%New()
 $$$ThrowOnError(m.XMLExportToStream(manifest))
 set info.Manifest=manifest
 do ..Obs(label,$$$ISOK(svc.PublishModule(info)))
}
ClassMethod Catalog(label,flag)
{
 set d=##class(%IPM.Repo.Oras.Definition).%New(),d.URL="http://oras:5000",svc=d.GetPackageService()
 set c=##class(%IPM.Repo.SearchCriteria).%New(),c.Name="platform-filter-test",c.VersionExpression="*",c.AllVersions=flag
 set list=svc.ListModules(c)
 do ..Obs(label_"-service-count",list.Count())
 if list.Count() {
  set first=list.GetAt(1)
  do ..Obs(label_"-first",+first.VersionString),..Obs(label_"-current",first.PlatformVersions.Find($$$CurrentPlatformVersion)>0),..Obs(label_"-bad",first.PlatformVersions.Find("9999.1")>0)
  do ..Obs(label_"-all-has-v2",first.AllVersions["2.0.0")
  for i=1:1:list.Count() {set item=list.GetAt(i) write !,"RAW-SERVICE|",label,"|",item.VersionString,"|",item.FormatPlatformVersions(),"|",item.AllVersions,!}
 }
 set sc=##class(%IPM.Repo.Utils).SearchRepositoriesForModule(c,.found)
 do ..Obs(label_"-manager-ok",$$$ISOK(sc)),..Obs(label_"-manager-count",found.Count())
 if found.Count() {set first=found.GetAt(1) do ..Obs(label_"-manager-first",+first.VersionString)}
}
ClassMethod Display(label,all)
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT Version,AllVersions,PlatformVersions FROM %IPM_Utils.Module_GetModuleList(?,?) WHERE Name = ?","oras",all,"platform-filter-test")
 set found=rs.%Next()
 do ..Obs(label_"-row",found)
 if found {do ..Obs(label_"-version",+rs.%Get("Version")),..Obs(label_"-current",rs.%Get("PlatformVersions")=$$$CurrentPlatformVersion),..Obs(label_"-all-v2",rs.%Get("AllVersions")["2.0.0")}
}
ClassMethod RawTags(label)
{
 set client=##class(%IPM.Repo.Oras.PackageService).GetClient("http://oras:5000","","","","")
 set raw=##class(%IPM.Repo.Oras.PackageService).GetAllTags("http://oras:5000","platform-filter-test","",client)
 write !,"RAW-TAGS|",label,"|",raw,!
 do ..Obs(label_"-v1-current",raw[("1.0.0"_$$$OrasTagPlatformSeparator_$$$CurrentPlatformVersion))
 do ..Obs(label_"-v2-bad",raw["2.0.0__9999.1")
 set sorted=##class(%IPM.Repo.Oras.PackageService).AggregatePlatformVersions($listfromstring(raw,", "),.platforms)
 do ..Obs(label_"-sorted-v2-first",$listget(sorted,1)="2.0.0")
 do ..Obs(label_"-v1-has-current",$listfind(platforms("1.0.0"),$$$CurrentPlatformVersion)>0)
 do ..Obs(label_"-v1-has-bad",$listfind(platforms("1.0.0"),"9999.1")>0)
}
ClassMethod Codec()
{
 set semver="1.2.3-alpha+build.7",encoded=$$$Semver2Tag(semver),raw=encoded_$$$OrasTagPlatformSeparator_$$$CurrentPlatformVersion
 do ..Obs("codec-encode",encoded="1.2.3-alpha_build.7")
 set pieces=$listfromstring(raw,$$$OrasTagPlatformSeparator)
 do ..Obs("codec-roundtrip",$$$Tag2Semver($listget(pieces,1))=semver),..Obs("codec-platform",$listget(pieces,2)=$$$CurrentPlatformVersion)
}
ClassMethod BuildKeys()
{
 try {
  set a="1.0.0_a.9"_$$$OrasTagPlatformSeparator_$$$CurrentPlatformVersion,z="1.0.0_z.1"_$$$OrasTagPlatformSeparator_$$$CurrentPlatformVersion
  set tags=$listbuild(a,z),py=##class(%SYS.Python).ToList(tags)
  set ordered=##class(%IPM.Utils.EmbeddedPython).FromPythonList(##class(%IPM.Utils.EmbeddedPython).SortOCITags(py,1))
  set aggregated=##class(%IPM.Repo.Oras.PackageService).AggregatePlatformVersions(tags,.platforms)
  do ..Obs("build-keys-exception",0),..Obs("build-tags-z-first",$listget(ordered,1)=z)
  do ..Obs("build-aggregate-z-first",$listget(aggregated,1)="1.0.0_z.1"),..Obs("build-aggregate-a-first",$listget(aggregated,1)="1.0.0_a.9")
  write !,"RAW-BUILD-KEYS|",$listtostring(aggregated,", "),!
 } catch ex {do ..Obs("build-keys-exception",1) write !,"RAW-BUILD-ERROR|",ex.DisplayString(),!}
}
ClassMethod Run()
{
 kill ^Iter15Error
 do ..Codec()
 do ..Cmd("repo-clear","repo -delete-all"),..Cmd("repo-create","repo -o -name oras -url http://oras:5000 -publish 1")
 do ..Package("v1","/tmp/iter15/v1")
 do ..Cmd("publish-v1","publish platform-filter-test -only -r oras"),..Cmd("remove-v1","uninstall platform-filter-test")
 do ..Package("v2","/tmp/iter15/v2")
 do ..PublishPlatform("publish-v2-bad","/tmp/iter15/v2","9999.1")
 do ..Cmd("remove-v2","uninstall platform-filter-test")
 do ..RawTags("mixed"),..Catalog("normal",0),..Catalog("all",1),..Catalog("deps",""),..Display("display-normal",0),..Display("display-all",1)
 do ..Cmd("install-mixed","install oras/platform-filter-test")
 do ..Obs("installed-version",..Version()),..Obs("installed-payload",..PublicVersion())
 do ..Cmd("remove-installed","uninstall platform-filter-test")
 do ..Cmd("range-load","load -nodev /tmp/iter15/modules/consumer/range")
 do ..Obs("range-dependency",..Version()),..Obs("range-payload",..PublicVersion()),..Obs("range-row",##class(%IPM.Storage.Module).NameExists("platform-filter-consumer"))
 do ..Cmd("range-remove","uninstall platform-filter-consumer"),..Cmd("range-remove-dependency","uninstall platform-filter-test")
 do ..Cmd("pinned-load","load -nodev /tmp/iter15/modules/consumer/pinned")
 do ..Obs("pinned-error-satisfactory",$get(^Iter15Error("pinned-load"))["Could not find satisfactory version")
 do ..Obs("pinned-error-name",$get(^Iter15Error("pinned-load"))["platform-filter-test")
 do ..Obs("pinned-dependency",..Version()),..Obs("pinned-payload",..PublicVersion()),..Obs("pinned-row",##class(%IPM.Storage.Module).NameExists("platform-filter-consumer-pinned"))
 do ..Package("v1","/tmp/iter15/v1-second","v1-second")
 do ..PublishPlatform("publish-v1-bad","/tmp/iter15/v1-second","9999.1")
 do ..Cmd("remove-second-v1","uninstall platform-filter-test")
 do ..RawTags("same-version"),..Catalog("same-version",0)
 do ..Cmd("install-same-version","install oras/platform-filter-test")
 do ..Obs("same-version-record",..Version()),..Obs("same-version-payload",..PublicVersion())
 do ..BuildKeys()
 do ..Obs("final-tlevel",$tlevel)
}
}
