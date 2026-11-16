/// Native checks NC-REPO-1..13 (module 06). ORAS checks use the shared zot at http://oras:5000 with
/// nc-repo- prefixed registry namespaces and module names only.
Class Uplift.Repo.Reg Extends Uplift.Repo.Util
{

Parameter MAPNS = "NCMAP";

ClassMethod Def(name As %String) As %IPM.Repo.Definition
{
    quit ##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(name)
}

/// Does the stored registry data contain this text anywhere?
ClassMethod StoredContains(text As %String) As %Boolean
{
    set g = "^IPM.Repo.DefinitionD", found = 0
    for {
        set g = $query(@g)
        quit:g=""
        quit:$qsubscript(g, 0)'="^IPM.Repo.DefinitionD"
        set v = @g
        set flat = $select($listvalid(v):$listtostring(v, "~", 1), 1:v)
        if flat [ text { set found = 1 quit }
    }
    quit found
}

/// NC-REPO-1: publish with no -r and no registry holding the publish flag.
ClassMethod Repo1()
{
    do ..Reset()
    set kv("repo") = $system.Status.IsOK(..Run("repo -r -n nc-repo-a -url http://127.0.0.1:9/ -token t", .o))
    set kv("deploymentEnabled") = ..Def("nc-repo-a").DeploymentEnabled
    set kv("load") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-repo-pub/", .o))
    set rs = ##class(%IPM.Repo.Http.Definition).PublishingServerFunc()
    set kv("publishingServerPick") = $select(rs.%Next():rs.Name, 1:"<none>")
    set sc = ..Run("publish nc-repo-pub -v", .out)
    do ..Status(sc, .kv)
    set kv("printedPublishTo") = ..Has(out, "Publish to: http://127.0.0.1:9/")
    set kv("printedNoServer") = ..Has(out_$get(kv("error")), "No server found to publish to")
    do ..Emit("nc-repo-1-publish-no-flag", .kv)
}

/// NC-REPO-3: changing the address of ORAS and remote registries; are the credentials kept?
ClassMethod Repo3()
{
    do ..Reset()
    for kind = "o", "r" {
        set n = "nc-repo-"_kind_"3"
        set h1 = $select(kind="o":"nc-host-a", 1:"nc-host-c"), h2 = $select(kind="o":"nc-host-b", 1:"nc-host-d")
        set kv(kind_".create") = $system.Status.IsOK(..Run("repo -"_kind_" -n "_n_" -url http://"_h1_":5000 -username u -password p", .o))
        set d = ..Def(n)
        set kv(kind_".userBefore") = d.Username
        set kv(kind_".passwordSetBefore") = (d.Password '= "")
        kill d
        set kv(kind_".change") = $system.Status.IsOK(..Run("repo -n "_n_" -url http://"_h2_":5000", .o))
        set d = ..Def(n)
        set kv(kind_".urlAfter") = d.URL
        set kv(kind_".userAfter") = d.Username
        set kv(kind_".passwordSetAfter") = (d.Password '= "")
        kill d
    }
    // a remote registry with the address an ORAS registry already holds
    set sc = ..Run("repo -r -n nc-repo-r3b -url http://nc-host-b:5000", .o)
    do ..Status(sc, .kv, "crossKindSameUrl.")
    do ..Emit("nc-repo-3-url-change-credentials", .kv)
}

/// NC-REPO-4: -token-auth-method for remote (and ORAS as a control).
ClassMethod Repo4()
{
    do ..Reset()
    set kv("r.create") = $system.Status.IsOK(..Run("repo -r -n nc-repo-r4 -url http://127.0.0.1:19/ -token t -token-auth-method bearer", .o))
    set d = ..Def("nc-repo-r4")
    set kv("r.stored") = d.TokenAuthMethod
    set req = d.GetPackageService().GetHttpRequest()
    set kv("r.headerApikey") = req.GetHeader("APIKEY")
    set kv("r.headerAuthorization") = req.GetHeader("AUTHORIZATION")
    kill req, d
    set kv("o.create") = $system.Status.IsOK(..Run("repo -o -n nc-repo-o4 -url http://127.0.0.1:29 -token t -token-auth-method bearer", .o))
    set kv("o.stored") = ..Def("nc-repo-o4").TokenAuthMethod
    do ..Emit("nc-repo-4-token-auth-method", .kv)
}

/// NC-REPO-5: -sslconfig and -publish-url.
ClassMethod Repo5()
{
    do ..Reset()
    set sc = ..Run("repo -r -n nc-repo-r5 -url https://nc-example.invalid/ -sslconfig NcCfg -publish-url http://nc-example.invalid/ncpubpath", .o)
    do ..Status(sc, .kv, "create.")
    set d = ..Def("nc-repo-r5")
    set req = d.GetPackageService().GetHttpRequest()
    set kv("requestSSLConfiguration") = req.SSLConfiguration
    set kv("requestHttps") = req.Https
    set kv("storedHasNcCfg") = ..StoredContains("NcCfg")
    set kv("storedHasPublishUrl") = ..StoredContains("ncpubpath")
    new $namespace
    set $namespace = "%SYS"
    set kv("sslConfigNcCfgExists") = ##class(Security.SSLConfigs).Exists("NcCfg")
    set kv("sslConfigHostExists") = ##class(Security.SSLConfigs).Exists("nc-example.invalid")
    do ..Emit("nc-repo-5-sslconfig-publish-url", .kv)
}

ClassMethod SettingNodes(key As %String, ByRef kv, prefix As %String)
{
    new $namespace
    set $namespace = "%SYS"
    set kv(prefix_"value") = $get(^IPM.settings(key), "<undef>")
    set kv(prefix_"default") = $get(^IPM.settings(key, "default"), "<undef>")
}

/// NC-REPO-6: config delete on a key with no seeded default.
ClassMethod Repo6()
{
    set key = $parameter("%IPM.Repo.UniversalSettings", "HistoryRetain")
    do ..SettingNodes(key, .kv, "fresh.")
    do ..Run("config set HistoryRetain 30", .o)
    do ..SettingNodes(key, .kv, "after30.")
    do ..Run("config set HistoryRetain 1", .o)
    do ..SettingNodes(key, .kv, "after1.")
    set sc = ..Run("config delete HistoryRetain", .o)
    do ..Status(sc, .kv, "delete.")
    do ..SettingNodes(key, .kv, "afterDelete.")
    do ..Run("config get HistoryRetain", .out)
    set kv("getPrinted") = $zstrip($piece(out, "HistoryRetain:", 2), "<>W")
    do ..Emit("nc-repo-6-config-delete", .kv)
    // restore the factory state of this key
    new $namespace
    set $namespace = "%SYS"
    kill ^IPM.settings(key)
}

ClassMethod Dest(ns As %String, glo As %String) As %String
{
    quit ##class(%SYS.Namespace).GetGlobalDest(ns, glo)
}

/// NC-REPO-7: default modifiers set in USER, read in a second IPM namespace; plus the ^% global fact.
ClassMethod Repo7()
{
    do ..Run("default-modifiers -delete", .o)
    set kv("set") = $system.Status.IsOK(..Run("default-modifiers -set -DNcProbe=1", .o))
    set kv("destUser") = ..Dest("USER", "%IPM.DefaultModifiers")
    set kv("destMap") = ..Dest(..#MAPNS, "%IPM.DefaultModifiers")
    set kv("destSys") = ..Dest("%SYS", "%IPM.DefaultModifiers")
    new $namespace
    set $namespace = ..#MAPNS
    set kv("map.ipmMain") = ..ClassState("%IPM.Main")
    set kv("map.data") = $data(^%IPM.DefaultModifiers)
    set sc = ..Run("default-modifiers -get", .out)
    do ..Status(sc, .kv, "map.get.")
    set kv("map.getListsNcProbe") = ..Has(out, "-DNcProbe=1")
    set kv("map.getSaysNone") = ..Has(out, "No default modifiers currently set")
    set $namespace = "%SYS"
    set kv("sys.data") = $data(^%IPM.DefaultModifiers)
    set $namespace = "USER"
    do ..Run("default-modifiers -delete", .o)
    set $namespace = ..#MAPNS
    set kv("map.dataAfterUserDelete") = $data(^%IPM.DefaultModifiers)
    set $namespace = "USER"
    do ..Emit("nc-repo-7-default-modifiers-namespaces", .kv)
    kill kv
    // IRIS fact: a ^% global written in one namespace
    set ^%NCProbe = 1
    set kv("destUser") = ..Dest("USER", "%NCProbe")
    set kv("destMap") = ..Dest(..#MAPNS, "%NCProbe")
    set $namespace = ..#MAPNS
    set kv("seenInMap") = $data(^%NCProbe)
    set $namespace = "%SYS"
    set kv("seenInSys") = $data(^%NCProbe)
    kill ^%NCProbe
    set $namespace = "USER"
    do ..Emit("nc-repo-env-percent-global-shared", .kv)
}

/// NC-REPO-8: config with an unknown key or a bad value.
ClassMethod Repo8()
{
    set sc = ..Run("config set NoSuchKey 1", .out)
    do ..Status(sc, .kv, "setUnknown.")
    set kv("setUnknown.printedNotFound") = ..Has(out, "not found")
    set sc = ..Run("config get NoSuchKey", .out)
    do ..Status(sc, .kv, "getUnknown.")
    set sc = ..Run("config set TestReportFormat xml", .out)
    do ..Status(sc, .kv, "setBadFormat.")
    set kv("setBadFormat.printed") = $extract($zstrip(out, "<>W"), 1, 120)
    set kv("setBadFormat.valueAfter") = ##class(%IPM.Repo.UniversalSettings).GetValue("TestReportFormat")
    do ..Emit("nc-repo-8-config-errors", .kv)
}

/// NC-REPO-10 control: -type with a moniker works.
ClassMethod Repo10Control()
{
    do ..Reset()
    set sc = ..Run("repo -type remote -n nc-repo-x2 -url http://127.0.0.1:39/", .o)
    do ..Status(sc, .kv)
    set d = ..Def("nc-repo-x2")
    set kv("createdClass") = $select($isobject(d):d.%ClassName(1), 1:"<none>")
    do ..Emit("nc-repo-10-control-type-moniker", .kv)
}

ClassMethod Repo10After(sc As %Status)
{
    do ..Status(sc, .kv)
    set kv("created") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-repo-x")
    do ..Emit("nc-repo-10-invalid-type", .kv)
}

/// NC-REPO-11: folder registry sees new module folders only after repo is run again.
ClassMethod Repo11()
{
    do ..Reset()
    set dir = "/tmp/nc-repo-fs/"
    do ##class(%File).RemoveDirectoryTree(dir)
    do ##class(%File).CreateDirectoryChain(dir)
    set kv("copy1") = ##class(%File).CopyDir(..#FIX_"nc-fs1/", dir_"nc-fs1/", 1)
    set kv("repo") = $system.Status.IsOK(..Run("repo -fs -n nc-repo-f -path "_dir, .o))
    do ..Run("search", .out)
    set kv("s1.fs1") = ..Has(out, "nc-fs1"), kv("s1.fs2") = ..Has(out, "nc-fs2")
    set kv("copy2") = ##class(%File).CopyDir(..#FIX_"fsrepo-extra/nc-fs2/", dir_"nc-fs2/", 1)
    do ..Run("search", .out)
    set kv("s2.fs1") = ..Has(out, "nc-fs1"), kv("s2.fs2") = ..Has(out, "nc-fs2")
    set sc = ..Run("install nc-fs2", .out)
    do ..Status(sc, .kv, "installBeforeRescan.")
    set kv("rescan") = $system.Status.IsOK(..Run("repo -n nc-repo-f -path "_dir, .o))
    do ..Run("search", .out)
    set kv("s3.fs1") = ..Has(out, "nc-fs1"), kv("s3.fs2") = ..Has(out, "nc-fs2")
    set sc = ..Run("install nc-fs2", .out)
    do ..Status(sc, .kv, "installAfterRescan.")
    set kv("fs2Installed") = ..Installed("nc-fs2")
    do ..Emit("nc-repo-11-folder-cache", .kv)
    do ..Run("uninstall nc-fs2", .o)
    do ##class(%File).RemoveDirectoryTree(dir)
}

/// NC-REPO-13: -password-env stores the value, not the variable name.
ClassMethod Repo13()
{
    do ..Reset()
    set os = ##class(%SYS.Python).Import("os")
    do os.putenv("NC_PW", "nc-secret-pw")
    do os.putenv("NC_TOK", "nc-secret-tok")
    set kv("envVisible") = ($system.Util.GetEnviron("NC_PW") = "nc-secret-pw")
    set sc = ..Run("repo -r -n nc-repo-r13 -url http://127.0.0.1:49/ -username u -password-env NC_PW", .out)
    do ..Status(sc, .kv, "pw.")
    set kv("pw.storedPlain") = ..StoredContains("nc-secret-pw")
    set kv("pw.storedVarName") = ..StoredContains("NC_PW")
    set kv("pw.listShowsValue") = ..Has(out, "nc-secret-pw")
    set sc = ..Run("repo -o -n nc-repo-o13 -url http://127.0.0.1:59 -token-env NC_TOK", .out)
    do ..Status(sc, .kv, "tok.")
    set kv("tok.storedPlain") = ..StoredContains("nc-secret-tok")
    set kv("tok.listShowsValue") = ..Has(out, "nc-secret-tok")
    do ..Emit("nc-repo-13-secret-at-rest", .kv)
}

ClassMethod RunAll()
{
    for m = "Repo1", "Repo3", "Repo4", "Repo5", "Repo6", "Repo7", "Repo8", "Repo10Control", "Repo11", "Repo13" {
        try {
            do $classmethod("Uplift.Repo.Reg", m)
        } catch e {
            write !, "OBS|error-reg-"_m_"|error="_..Safe(e.DisplayString())_";", !
        }
        if $tlevel { trollback }
        new $namespace
        set $namespace = "USER"
    }
    do ..Reset()
}

}
