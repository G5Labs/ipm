Class Iter7.Probe
{
ClassMethod Obs(k,v) { write !,"OBS|",k,"|",v,! }
ClassMethod Ref(name="/iter7") As %IPM.Storage.ResourceReference
{
 set m=##class(%IPM.Storage.Module).%New(),m.Name="iter7",m.Root="/tmp/iter7/root/"
 set r=##class(%IPM.Storage.ResourceReference).%New(),r.Name=name,r.Module=m
 return r
}
ClassMethod Run()
{
 set c("A")="{$B}",c("B")="x",failed=0
 try { set v=##class(%IPM.Storage.ModuleSetting.Default).EvaluateAttribute("{$A}",.c) } catch e { set failed=e.Name="<REGULAR EXPRESSION>" write !,"RAW|forward-error|",e.DisplayString(),! }
 do ..Obs("param-forward-fails",failed)
 kill c
 set c("A")="x",c("B")="{$A}",failed=0
 try { set v=##class(%IPM.Storage.ModuleSetting.Default).EvaluateAttribute("{$B}",.c) } catch e { set failed=e.Name="<REGULAR EXPRESSION>" write !,"RAW|backward-error|",e.DisplayString(),! }
 do ..Obs("param-backward-fails",failed)
 kill c
 set c("A")="x",c("B")="y"
 set v=##class(%IPM.Storage.ModuleSetting.Default).EvaluateAttribute("{$A}/{$B}",.c)
 do ..Obs("param-ordinary",v="x/y")
 set x=##class(Iter7.Options).CommaXSDToLogical("a,b|c"),y=##class(Iter7.Options).PipeXSDToLogical("a,b|c")
 do ..Obs("comma-count",$listlength(x)),..Obs("comma-first-length",$length($list(x,1)))
 do ..Obs("pipe-count",$listlength(y)),..Obs("pipe-first-length",$length($list(y,1)))
 set r=..Ref(),a=##class(Iter7.CSP).%New(r)
 set sc=a.OnBeforePhase("Initialize",.p)
 do ..Obs("csp-default",^Iter7Directory="/tmp/iter7/root/iter7/")
 set r=..Ref() do r.Attributes.SetAt("Iter7.Dispatch","DispatchClass")
 set a=##class(Iter7.CSP).%New(r),sc=a.OnBeforePhase("Initialize",.p)
 do ..Obs("csp-sentinel",^Iter7Directory="<irrelevant>"),..Obs("csp-final-empty",a.Directory="")
 set r=..Ref()
 do r.Attributes.SetAt("Dispatch","DispatchClass"),r.Attributes.SetAt("wsgi.py","WSGIAppLocation"),r.Attributes.SetAt("/path/","Path"),r.Attributes.SetAt("/dir/","Directory")
 do ..Obs("detail-dispatch",##class(Iter7.Summary).Detail(r)="dispatch Dispatch")
 do r.Attributes.SetAt("","DispatchClass")
 do ..Obs("detail-wsgi",##class(Iter7.Summary).Detail(r)="wsgi wsgi.py")
 do r.Attributes.SetAt("","WSGIAppLocation")
 do ..Obs("detail-path",##class(Iter7.Summary).Detail(r)="files at /path/")
 do r.Attributes.SetAt("","Path")
 do ..Obs("detail-directory",##class(Iter7.Summary).Detail(r)="files at /dir/")
 new $namespace
 set ns=$namespace,$namespace="%SYS",props("NameSpace")=ns
 $$$ThrowOnError(##class(Security.Applications).Create("/iter7-status",.props))
 set $namespace=ns
 set r=..Ref("/iter7-status")
 do r.Attributes.SetAt("/iter7-status","Name"),r.Attributes.SetAt(ns,"NameSpace")
 set a=##class(%IPM.ResourceProcessor.WebApplication).%New(r),sc=a.OnBeforePhase("Activate",.p)
 do ..Obs("app-equal-ok",$$$ISOK(sc))
 do r.Attributes.SetAt("ITER7-MISSING-NAMESPACE","NameSpace")
 set a=##class(%IPM.ResourceProcessor.WebApplication).%New(r),sc=a.OnBeforePhase("Activate",.p)
 do ..Obs("app-future-namespace-ok",$$$ISOK(sc))
 do r.Attributes.SetAt(ns,"NameSpace"),r.Attributes.SetAt(":ITER7-ABSENT-ROLE","MatchRoles")
 set a=##class(%IPM.ResourceProcessor.WebApplication).%New(r),sc=a.OnBeforePhase("Activate",.p)
 write !,"RAW|modify-error|",$system.Status.GetErrorText(sc),!
 do ..Obs("app-changed-fails",$$$ISERR(sc)),..Obs("app-restored-ns",$namespace=ns)
 set t=##class(%IPM.Storage.ModuleTemplate).NewTemplate("/tmp/iter7/","iter7","1.0.0")
 set sc=t.ValidateCorsOrigin("http://example.com,https://example.org")
 do ..Obs("cors-valid",$$$ISOK(sc))
 set sc=t.ValidateCorsOrigin("http://example.com,this is not an origin,https://example.org")
 write !,"RAW|cors-error|",$system.Status.GetErrorText(sc),!
 do ..Obs("cors-invalid",$$$ISOK(sc)),..Obs("cors-restored-ns",$namespace=ns)
 set m=##class(%IPM.Storage.Module).%New(),m.Name="iter7-hash",m.VersionString="1.0.0",m.Root="/tmp/iter7/"
 $$$ThrowOnError(m.%Save())
 set sc=##class(%IPM.Storage.FileHash).StampOneFileWithHash(m.Name,"src/a.cls","abc")
 do ..Obs("hash-initial",$$$ISOK(sc))
 set sc=##class(%IPM.Storage.FileHash).StampOneFileWithHash(m.Name,"src/a.cls","")
 do ..Obs("hash-empty-fails",$$$ISERR(sc))
 set h=##class(%IPM.Storage.FileHash).OpenByPath(m.Name,"src/a.cls",.s)
 do ..Obs("hash-old-kept",h.Hash="abc")
 set sc=##class(%IPM.Storage.FileHash).StampOneFileWithHash(m.Name,"src/a.cls","def")
 set h=##class(%IPM.Storage.FileHash).OpenByPath(m.Name,"src/a.cls",.s)
 do ..Obs("hash-new",h.Hash="def")
 set m=##class(%IPM.Storage.Module).%New(),m.Name="iter7-reload",m.Root="/tmp/iter7/missing/",m.DeveloperMode=0
 kill ^Iter7EnvCalls
 set l=##class(Iter7.Lifecycle).%New(m),sc=l.%Reload(.p)
 do ..Obs("reload-nondev-ok",$$$ISOK(sc)),..Obs("reload-nondev-env",$get(^Iter7EnvCalls))
 set m.DeveloperMode=1,l=##class(Iter7.Lifecycle).%New(m),sc=l.%Reload(.p)
 do ..Obs("reload-dev-fails",$$$ISERR(sc)),..Obs("reload-dev-env",$get(^Iter7EnvCalls))
 kill ^Iter7Reads
 do ##class(Iter7.Settings).PrintOne("Known")
 do ..Obs("config-known-reads",$get(^Iter7Reads))
 do ##class(Iter7.Settings).PrintOne("Unknown")
 do ..Obs("config-unknown-extra-reads",$get(^Iter7Reads)-1)
 set ^%SYS("CSP","Test")="old"
 set sc=##class(%IPM.Main).Shell("load /home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/system-setting/")
 do ..Obs("system-load",$$$ISOK(sc)),..Obs("system-value",$get(^%SYS("CSP","Test"),0))
 set sc=##class(%IPM.Main).Shell("uninstall system-setting")
 do ..Obs("system-uninstall",$$$ISOK(sc)),..Obs("system-value-after",$get(^%SYS("CSP","Test"),0))

}
}
