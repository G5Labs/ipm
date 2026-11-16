/// Measures the write set of IPM commands: which globals change, in which database.
/// A snapshot is a hash per global (walk with $query, CRC over subscripts and values) for every global in the
/// databases the namespace and %SYS use. Each command's write set is the list of globals whose hash changed.
Class Uplift.Probe.WriteSet
{

/// dbs(dir) = label of every database to scan
ClassMethod Databases(Output dbs)
{
    kill dbs
    set dbs("^/usr/irissys/mgr/iristemp/") = "IRISTEMP"
    set dbs("^/usr/irissys/mgr/") = "IRISSYS"
    // Web, CSP and security definitions live in IRISSECURITY (^SECURITY), not in %SYS's globals (review REVIEW-2 N1).
    set dbs("^/usr/irissys/mgr/irissecurity/") = "IRISSECURITY"
    set dbs("^/usr/irissys/mgr/irislocaldata/") = "IRISLOCALDATA"
    set dbs("^/usr/irissys/mgr/irisaudit/") = "IRISAUDIT"
    for ns = $namespace, "%SYS", "NCSEM" {
        continue:'##class(%SYS.Namespace).Exists(ns)
        for d = ##class(%SYS.Namespace).GetGlobalDest(ns, "X"), ##class(%SYS.Namespace).GetRoutineDest(ns, "X") {
            set:'$data(dbs(d)) dbs(d) = ns_"DB"
        }
    }
}

ClassMethod HashGlobal(gref As %String) As %Integer
{
    set h = 0, q = gref
    if $data(@q) # 2 { set h = $zcrc($get(@q), 7, h) }
    for {
        set q = $query(@q)
        quit:q=""
        set h = $zcrc(q_"="_$get(@q), 7, h)
    }
    quit h
}

/// snap(label, global) = hash
ClassMethod Snapshot(Output snap)
{
    kill snap
    do ..Databases(.dbs)
    set dir = ""
    for {
        set dir = $order(dbs(dir), 1, label)
        quit:dir=""
        set g = ""
        for {
            set g = $order(^$|("^"_dir)|GLOBAL(g))
            quit:g=""
            continue:g["UpliftWS"
            set snap(label, g) = ..HashGlobal("^|""^"_dir_"""|"_$extract(g, 2, *))
        }
    }
}

/// Run one shell command and print OBS|ws-<name>|written=<label:global,...>;status=...
ClassMethod Measure(name As %String, cmd As %String, setup As %String = "")
{
    if setup '= "" { xecute setup }
    do ..Snapshot(.before)
    set sc = ##class(%IPM.Main).Shell(cmd)
    do ..Snapshot(.after)
    set out = "", label = ""
    for {
        set label = $order(after(label))
        quit:label=""
        set g = ""
        for {
            set g = $order(after(label, g), 1, h)
            quit:g=""
            if $get(before(label, g)) '= h { set out = out_$listbuild(label_":"_g) }
        }
    }
    set label = ""
    for {
        set label = $order(before(label))
        quit:label=""
        set g = ""
        for {
            set g = $order(before(label, g))
            quit:g=""
            if '$data(after(label, g)) { set out = out_$listbuild(label_":"_g_"(killed)") }
        }
    }
    set kv("cmd") = $translate(cmd, ";|=", ",/:")
    set kv("status") = $select($system.Status.IsOK(sc): "ok", 1: "err")
    set kv("written") = $translate($listtostring(out, ","), ";|=", "::_")
    do ##class(Uplift.Probe.Env).Emit("ws-"_name, .kv)
}

/// Scratch namespace NCSEM with its own database (created outside any measurement).
ClassMethod MakeNamespace()
{
    new $namespace
    set $namespace = "%SYS"
    if ##class(Config.Namespaces).Exists("NCSEM") { quit }
    set dir = "/usr/irissys/mgr/ncsem/"
    do ##class(%File).CreateDirectoryChain(dir)
    $$$ThrowOnError(##class(SYS.Database).CreateDatabase(dir))
    set p("Directory") = dir
    $$$ThrowOnError(##class(Config.Databases).Create("NCSEM", .p))
    kill p set p("Globals") = "NCSEM", p("Routines") = "NCSEM"
    $$$ThrowOnError(##class(Config.Namespaces).Create("NCSEM", .p))
}

ClassMethod RunMore()
{
    do ##class(Uplift.Probe.Scenario).Reset()
    do ..MakeNamespace()
    set sync = "/tmp/ncsync/sync-test/"
    do ..Measure("load-dev-sync-fixture", "load -dev "_sync)
    do ..Measure("compile", "compile sync-test")
    do ..Measure("sync-no-change", "sync sync-test")
    set f = ##class(%Stream.FileCharacter).%New()
    do f.LinkToFile(sync_"src/cls/SyncTest/SuperClass.cls")
    set text = f.Read(1000000) kill f
    set f = ##class(%Stream.FileCharacter).%New()
    do f.LinkToFile(sync_"src/cls/SyncTest/SuperClass.cls")
    do f.Write($replace(text, "original", "edited")) do f.%Save() kill f
    do ..Measure("sync-after-edit", "sync sync-test")
    do ..Measure("install-up-dep", "install up-dep")
    do ..Measure("reinstall", "reinstall up-dep")
    do ..Measure("package", "package up-dep -path /tmp/ncpkg/")
    do ##class(%IPM.Main).Shell("repo -n ncsem-oras -o -url http://oras:5000 -publish 1")
    do ..Measure("publish", "publish up-dep -r ncsem-oras")
    do ..Measure("unpublish", "unpublish ncsem-oras/up-dep 1.0.0 -force")
    do ##class(%IPM.Main).Shell("repo -n ncsem-oras -delete")
    do ..Measure("enable-map", "enable -map -ns NCSEM")
    do ..Measure("unmap", "unmap -ns NCSEM")
    do ##class(Uplift.Probe.Scenario).Reset()
}

/// Review round: successful registry commands, update steps, forced and resource-bearing uninstalls.
ClassMethod RunReview()
{
    set data = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/"
    do ##class(Uplift.Probe.Scenario).Reset()
    do ##class(%IPM.Main).Shell("repo -n ws-upd -fs -path "_data_"update-test/")
    do ##class(%IPM.Main).Shell("repo -n ws-fc -fs -path "_data_"file-copy-test/")
    do ..Measure("repo-create-fs", "repo -n ws-new -fs -path /tmp/ncrepo/")
    do ..Measure("repo-modify", "repo -n ws-new -fs -path /tmp/ncrepo/ -snapshots 0")
    do ..Measure("repo-delete-ok", "repo -n ws-new -delete")
    do ##class(%IPM.Main).Shell("install update-simple 1.2.1")
    do ..Measure("update-with-steps", "update update-simple 2.1.1")
    do ..Measure("install-force-with-steps", "install -force update-simple 3.5.0")
    do ..Measure("install-permit-downgrade", "install update-simple 1.2.1 -DPermitDowngrade=1")
    do ##class(%IPM.Main).Shell("uninstall update-simple")
    do ##class(%IPM.Main).Shell("install update-simple 1.2.1")
    do ..Measure("update-dev", "update update-simple 2.1.1 -dev")
    do ##class(%IPM.Main).Shell("load /tmp/uplift/fixtures/up-parent-ok/")
    do ..Measure("uninstall-force-needed", "uninstall up-dep -force")
    do ..Measure("load-webapp", "load "_data_"rest-app/")
    do ..Measure("uninstall-webapp", "uninstall rest-demo")
    do ##class(%IPM.Main).Shell("install fc-no-dep")
    do ..Measure("uninstall-filecopy", "uninstall fc-no-dep")
    do ##class(%IPM.Main).Shell("repo -n ws-upd -delete")
    do ##class(%IPM.Main).Shell("repo -n ws-fc -delete")
    do ##class(Uplift.Probe.Scenario).Reset()
}

ClassMethod RunWebApp()
{
    set data = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/"
    do ##class(Uplift.Probe.Scenario).Reset()
    do ..Measure("load-webapp", "load "_data_"rest-app/")
    do ..Measure("uninstall-webapp", "uninstall rest-demo")
    do ##class(Uplift.Probe.Scenario).Reset()
}

ClassMethod RunAll()
{
    set fix = "/tmp/uplift/fixtures/", simplest = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/simplest-module/"
    do ##class(Uplift.Probe.Scenario).Reset()
    do ..Measure("load-ok", "load "_simplest_"1.0.0/")
    do ..Measure("load-same-version", "load "_simplest_"1.0.0/")
    do ..Measure("load-refused-downgrade", "load "_simplest_"1.0.0/", "do ##class(%IPM.Main).Shell(""load "_simplest_"2.0.0/"")")
    do ..Measure("load-activate-fails", "load "_fix_"up-res-fail/")
    do ..Measure("load-dev-activate-fails", "load -dev "_fix_"up-res-fail/")
    do ..Measure("uninstall", "uninstall up-res-fail")
    do ..Measure("load-notxn-activate-fails", "load "_fix_"up-res-fail/ -DNoTransaction=1")
    do ..Measure("uninstall-2", "uninstall up-res-fail")
    do ..Measure("load-with-dependency", "load "_fix_"up-parent-ok/")
    do ..Measure("uninstall-recursive", "uninstall up-parent-ok -r")
    do ..Measure("install-from-registry", "install up-dep")
    do ..Measure("update-with-path", "update simplest-module -path "_simplest_"3.0.0/")
    do ..Measure("list-installed", "list-installed")
    do ..Measure("history-find", "history find")
    do ..Measure("history-delete-filter", "history delete -Daction=install")
    do ..Measure("repo-add-fs", "repo -n ws-repo -fs -path "_fix_"repo/")
    do ..Measure("repo-delete", "repo -n ws-repo -delete")
    do ..Measure("config-set", "config set DefaultLogEntryLimit 7")
    do ..Measure("config-delete", "config delete DefaultLogEntryLimit")
    do ..Measure("default-modifiers-set", "default-modifiers -set -DUpliftWsProbe=1")
    do ..Measure("default-modifiers-clear", "default-modifiers -delete")
    do ..Measure("search", "search")
    do ..Measure("orphans", "orphans -type CLS")
    do ..Measure("version", "version")
    do ##class(Uplift.Probe.Scenario).Reset()
}

}
