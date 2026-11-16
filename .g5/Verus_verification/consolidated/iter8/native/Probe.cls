Include (%IPM.Common, %sySecurity)
Class Iter8.Probe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Cmd(k,cmd) As %Boolean
{
 set sc=##class(%IPM.Main).Shell(cmd)
 do ..Obs(k,$$$ISOK(sc))
 if $$$ISERR(sc) { write !,"RAW|",k,"|",$system.Status.GetErrorText(sc),! }
 return $$$ISOK(sc)
}
ClassMethod Root() As %String { return "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/" }
ClassMethod App(name,field) As %String
{
 new $namespace
 set $namespace="%SYS"
 $$$ThrowOnError(##class(Security.Applications).Get(name,.props))
 return $get(props(field))
}
ClassMethod Exists(name) As %Boolean
{
 new $namespace
 set $namespace="%SYS"
 return ##class(Security.Applications).Exists(name)
}
ClassMethod AdminCreate(name,ByRef props)
{
 new $namespace
 set $namespace="%SYS"
 $$$ThrowOnError(##class(Security.Applications).Create(name,.props))
}
ClassMethod AdminDelete(name)
{
 new $namespace
 set $namespace="%SYS"
 $$$ThrowOnError(##class(Security.Applications).Delete(name))
}
ClassMethod ReadFile(path) As %String
{
 set f=##class(%Stream.FileCharacter).%New()
 $$$ThrowOnError(f.LinkToFile(path))
 return f.Read(32000)
}
ClassMethod Generated()
{
 set ns=$namespace,rest("NameSpace")=ns,rest("DispatchClass")="GenFixture.REST",rest("AutheEnabled")=$$$AutheCache,rest("Recurse")=0
 set dest=##class(%File).NormalizeDirectory("csp/iter8-csp",$system.Util.InstallDirectory())
 set csp("NameSpace")=ns,csp("Path")=dest,csp("AutheEnabled")=$$$AutheUnauthenticated
 do ..AdminCreate("/iter8-rest",.rest),..AdminCreate("/iter8-csp",.csp)
 $$$ThrowOnError(##class(%IPM.Utils.File).CopyDir(..Root()_"generate/web-apps/","/tmp/iter8/generated/"))
 set t=##class(%IPM.Storage.ModuleTemplate).NewTemplate("/tmp/iter8/generated/","iter8-generated","1.0.0")
 set jwt("enableJWT")=1,jwt("JWTAccessTokenTO")=120,jwt("JWTRefreshTokenTO")=1800
 do t.SetJWTProps("/iter8-rest",.jwt)
 $$$ThrowOnError(t.AddWebApps("/iter8-rest,/iter8-csp",.files))
 do ..Obs("gen-csp-only",$listtostring(files)="/iter8-csp")
 do t.SetSourcePathForCSPApp("/iter8-csp","web/")
 do t.ProcessResources()
 $$$ThrowOnError(t.SaveFile("/tmp/iter8/generated/"))
 $$$ThrowOnError(##class(%XML.TextReader).ParseFile("/tmp/iter8/generated/module.xml",.reader))
 set expr=0,copy=0
 while reader.Read() {
  continue:(reader.NodeType'="element")
  set element=reader.LocalName
  for i=1:1:reader.AttributeCount {
   do reader.MoveToAttributeIndex(i)
   if (element="WebApplication")&&(reader.LocalName="Path") { set expr=(reader.Value["{$cspdir}") }
   if (element="FileCopy")&&(reader.LocalName="Target") { set copy=(reader.Value["{$cspdir}") }
  }
 }
 do ..Obs("gen-path-expression",expr),..Obs("gen-copy-expression",copy)
 do ..AdminDelete("/iter8-rest"),..AdminDelete("/iter8-csp")
 do ..Cmd("gen-load","load /tmp/iter8/generated/")
 do ..Obs("gen-jwt-enabled",..App("/iter8-rest","JWTAuthEnabled"))
 do ..Obs("gen-jwt-access",..App("/iter8-rest","JWTAccessTokenTimeout"))
 do ..Obs("gen-jwt-refresh",..App("/iter8-rest","JWTRefreshTokenTimeout"))
 do ..Obs("gen-path",..App("/iter8-csp","Path")=dest)
 do ..Obs("gen-copied-file",##class(%File).Exists(dest_"index.html"))
 do ..Obs("gen-copied-bytes",..ReadFile(dest_"index.html")=..ReadFile("/tmp/iter8/generated/web/index.html"))
}
ClassMethod WebRoundtrip()
{
 do ..Cmd("repo-oras","repo -o -name oras -url http://oras:5000 -publish 1")
 do ..Cmd("web-load","load "_..Root()_"webapp-export-test/")
 set before=..App("/webapp-export-test","Path")
 do ..Cmd("web-publish","publish webapp-export-test -r oras")
 do ..Cmd("web-uninstall","uninstall webapp-export-test")
 do ..Obs("web-absent",'..Exists("/webapp-export-test"))
 do ..Cmd("web-install","install oras/webapp-export-test")
 do ..Obs("web-path-same",..App("/webapp-export-test","Path")=before)
}
ClassMethod Catalog()
{
 do ..Cmd("catalog-repo","repo -name simplest-module -fs -path "_..Root()_"simplest-module/")
 set c=##class(%IPM.Repo.SearchCriteria).%New(),c.Name="simplest-module",c.AllVersions=1
 $$$ThrowOnError(##class(%IPM.Repo.Utils).SearchRepositoriesForModule(c,.list))
 do ..Obs("catalog-count",list.Count())
 set item=list.GetAt(1)
 do ..Obs("catalog-selected",+item.VersionString),..Obs("catalog-all",item.AllVersions="3.0.0, 2.0.0, 1.0.0")
 set c.VersionExpression="<3.0.0"
 $$$ThrowOnError(##class(%IPM.Repo.Utils).SearchRepositoriesForModule(c,.list))
 set item=list.GetAt(1)
 do ..Obs("catalog-limited-count",list.Count()),..Obs("catalog-limited-selected",+item.VersionString),..Obs("catalog-limited-all",item.AllVersions="2.0.0, 1.0.0")
}
ClassMethod Listed(ByRef platforms) As %Boolean
{
 set rs=##class(%SQL.Statement).%ExecDirect(,"SELECT PlatformVersions FROM %IPM_Utils.Module_GetModuleList(?,?) WHERE Name = ?","oras",0,"platform-filter-test")
 if 'rs.%Next() { set platforms="" return 0 }
 set platforms=rs.%Get("PlatformVersions")
 return 1
}
ClassMethod PublishBad()
{
 set module=##class(%IPM.Storage.Module).NameOpen("platform-filter-test",,.sc)
 $$$ThrowOnError(sc)
 set def=##class(%IPM.Repo.Oras.Definition).%New(),def.URL="http://oras:5000",svc=def.GetPublishService()
 set info=##class(%IPM.Repo.Remote.ModuleInfo).%New(),info.Name="platform-filter-test",info.VersionString=module.VersionString,info.Deployed=1,info.PlatformVersion="9999.1"
 set f=##class(%Stream.FileBinary).%New(),f.Filename="/tmp/iter8/platform.tgz"
 $$$ThrowOnError(info.Payload.CopyFrom(f))
 set manifest=##class(%Stream.GlobalCharacter).%New()
 $$$ThrowOnError(module.XMLExportToStream(manifest))
 set info.Manifest=manifest
 do ..Obs("platform-publish-incompatible",$$$ISOK(svc.PublishModule(info)))
}
ClassMethod Platform()
{
 do ..Cmd("platform-load","load -nodev "_..Root()_"platform-filter/test/v1/")
 do ..Cmd("platform-package","package platform-filter-test -only -path /tmp/iter8/platform")
 do ..Cmd("platform-publish-compatible","publish platform-filter-test -only -r oras")
 do ..Obs("platform-visible",..Listed(.platforms)),..Obs("platform-reported",platforms=$$$CurrentPlatformVersion)
 do ..Cmd("platform-remove-local","uninstall platform-filter-test")
 do ..Cmd("platform-install-compatible","install oras/platform-filter-test")
 do ..Obs("platform-code-version",$classmethod("PlatformFilterTest.Public","Version")="1.0.0")
 do ..Cmd("platform-remove-again","uninstall platform-filter-test")
 do ..Cmd("platform-unpublish","unpublish oras/platform-filter-test all -force")
 do ..Cmd("platform-reload","load -nodev "_..Root()_"platform-filter/test/v1/")
 do ..Cmd("platform-repackage","package platform-filter-test -only -path /tmp/iter8/platform")
 do ..PublishBad()
 do ..Cmd("platform-remove-bad-local","uninstall platform-filter-test")
 do ..Obs("platform-incompatible-visible",..Listed(.platforms))
 do ..Cmd("platform-incompatible-install","install oras/platform-filter-test")
}
ClassMethod WheelResource(moduleName,ByRef handled) As %Status
{
 set m=##class(%IPM.Storage.Module).%New(),m.Name=moduleName,m.Root="/tmp/iter8/"
 set r=##class(%IPM.Storage.ResourceReference).%New(),r.Name="missing-1.0-py3-none-any.whl",r.Module=m
 do r.Attributes.SetAt("missing-1.0-py3-none-any.whl","Name"),r.Attributes.SetAt(".","Directory")
 set p=##class(%IPM.ResourceProcessor.PythonWheel).%New(r)
 return p.OnPhase("Initialize",.params,.handled)
}
ClassMethod PythonVersion() As %String
{
 set lib=##class(%SYS.Python).Import("importlib"),metadata=lib."import_module"("importlib.metadata")
 return metadata."version"("lune")
}
ClassMethod Python()
{
 do ..Cmd("wheel-load","load /tmp/iter8/wheelmodule -bypass-py-deps")
 do ..Obs("wheel-version",..PythonVersion()="1.6.2")
 do ..Cmd("wheel-uninstall","uninstall iter8-wheel")
 do ..Obs("wheel-kept",..PythonVersion()="1.6.2")
 set sc=..WheelResource("iter8-other",.handled)
 do ..Obs("wheel-missing-other",$$$ISOK(sc)),..Obs("wheel-missing-other-handled",handled)
 set sc=..WheelResource("zpm",.handled)
 do ..Obs("wheel-missing-ipm",$$$ISOK(sc)),..Obs("wheel-missing-ipm-handled",handled)
}
ClassMethod External()
{
 do ..Cmd("external-load","load "_..Root()_"publish-external-name/0.0.1/")
 do ..Cmd("external-publish","publish default-name -use-ext -r oras")
 do ..Cmd("external-uninstall","uninstall default-name")
 do ..Cmd("external-install","install oras/external-name")
 do ..Obs("external-manifest-internal",##class(%IPM.Storage.Module).NameExists("default-name"))
 do ..Obs("external-manifest-external",##class(%IPM.Storage.Module).NameExists("external-name"))
}
}
