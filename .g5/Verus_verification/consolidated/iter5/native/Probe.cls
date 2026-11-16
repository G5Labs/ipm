/// Iteration 5 native observations. Each line OBS|case|field|value is parsed by run_native.py.
Class Iter5.Probe
{

ClassMethod Obs(case As %String, field As %String, value As %String)
{
    write !,"OBS|",case,"|",field,"|",$translate(value,$char(13,10),"  "),!
}

ClassMethod Cmd(case As %String, command As %String)
{
    set sc = ##class(%IPM.Main).Shell(command)
    do ..Obs(case, "ok", $system.Status.IsOK(sc))
    do ..Obs(case, "error", $select($system.Status.IsOK(sc):"", 1:$system.Status.GetErrorText(sc)))
}

/// Record whether a web application exists and the properties the cases compare.
ClassMethod App(case As %String, name As %String)
{
    new $namespace
    set $namespace = "%SYS"
    set exists = ##class(Security.Applications).Exists(name)
    do ..Obs(case, name_"|exists", exists)
    if exists {
        do ##class(Security.Applications).Get(name, .p)
        do ..Obs(case, name_"|Description", $get(p("Description")))
        do ..Obs(case, name_"|Timeout", $get(p("Timeout")))
        do ..Obs(case, name_"|NameSpace", $get(p("NameSpace")))
    }
}

ClassMethod Installed(case As %String, module As %String)
{
    do ..Obs(case, module_"|installed", ##class(%IPM.Storage.Module).NameExists(module))
}

/// Administrator actions outside IPM.
ClassMethod AdminCreate(name As %String, description As %String, timeout As %Integer)
{
    new $namespace
    set $namespace = "%SYS"
    set p("Description") = description, p("Timeout") = timeout, p("NameSpace") = "USER"
    do ##class(Security.Applications).Create(name, .p)
}

ClassMethod AdminModify(name As %String, description As %String, timeout As %Integer)
{
    new $namespace
    set $namespace = "%SYS"
    set p("Description") = description, p("Timeout") = timeout
    do ##class(Security.Applications).Modify(name, .p)
}

ClassMethod WebApps()
{
    set root = "/tmp/g5-iter5/"
    // W1: a fresh install creates the app in the current namespace.
    do ..Cmd("W1-install-a", "load "_root_"wa")
    do ..App("W1-install-a", "/csp/g5i5a")
    // W2: an administrator's app with the same name is adopted: declared keys overwritten, others kept.
    do ..AdminCreate("/csp/g5i5pre", "admin", 777)
    do ..App("W2-before", "/csp/g5i5pre")
    do ..Cmd("W2-install-b", "load "_root_"wb")
    do ..App("W2-install-b", "/csp/g5i5pre")
    // W3: uninstall of the adopting module deletes the administrator's app.
    do ..Cmd("W3-uninstall-b", "uninstall g5i5-web-b")
    do ..App("W3-uninstall-b", "/csp/g5i5pre")
    // W4: a second module declaring A's app name.
    do ..Cmd("W4-install-c", "load "_root_"wc")
    do ..Installed("W4-install-c", "g5i5-web-c")
    do ..App("W4-install-c", "/csp/g5i5a")
    // W5: reinstall after manual edits: declared key restored, undeclared key kept.
    do ..Cmd("W5-install-g", "load "_root_"wg")
    do ..AdminModify("/csp/g5i5g", "edited", 555)
    do ..Cmd("W5-reload-g", "load "_root_"wg")
    do ..App("W5-reload-g", "/csp/g5i5g")
    // W6: blocked uninstall of a module another module needs.
    do ..Cmd("W6-install-d", "load "_root_"wd")
    do ..Cmd("W6-uninstall-a", "uninstall g5i5-web-a")
    do ..Installed("W6-uninstall-a", "g5i5-web-a")
    do ..App("W6-uninstall-a", "/csp/g5i5a")
    do ..App("W6-uninstall-a", "/csp/g5i5d")
    // W7: the same with -force removes the module and leaves the dependent installed.
    do ..Cmd("W7-force-a", "uninstall g5i5-web-a -force")
    do ..Installed("W7-force-a", "g5i5-web-a")
    do ..Installed("W7-force-a", "g5i5-web-d")
    // W8: install that fails after Activate created the app.
    do ..Cmd("W8-install-e", "load "_root_"we")
    do ..Installed("W8-install-e", "g5i5-web-e")
    do ..App("W8-install-e", "/csp/g5i5e")
    // W8b: the same failure without a transaction.
    do ..Cmd("W8b-install-e-notxn", "load "_root_"we -DNoTransaction=1")
    do ..Installed("W8b-install-e-notxn", "g5i5-web-e")
    do ..App("W8b-install-e-notxn", "/csp/g5i5e")
    // W9: an attribute that is not a Security.Applications property.
    do ..Cmd("W9-install-f", "load "_root_"wf")
    do ..App("W9-install-f", "/csp/g5i5f")
    // W10: upgrading to a version whose manifest no longer declares the app.
    do ..Cmd("W10-upgrade-g", "load "_root_"wg2")
    do ..Obs("W10-upgrade-g", "version", ##class(%IPM.Storage.Module).NameOpen("g5i5-web-g").VersionString)
    do ..App("W10-upgrade-g", "/csp/g5i5g")
    do ..Cmd("W10-uninstall-g", "uninstall g5i5-web-g")
    do ..App("W10-uninstall-g", "/csp/g5i5g")
}

/// Iteration 4 challenge: a same-origin download URL that redirects to another origin.
ClassMethod Redirect()
{
    set svc = ##class(%IPM.Repo.Remote.PackageService).%New()
    set svc.Location = "http://localhost:18501/"
    set svc.Username = "fixture-user"
    set svc.Password = "fixture-pass"
    set svc.Token = "fixture-key"
    set ref = ##class(%IPM.Storage.ModuleInfo).%New()
    set ref.Name = "redirect"
    set ref.VersionString = "1.0.0"
    set ref.Deployed = 0
    try {
        set stream = svc.GetModule(ref)
        do ..Obs("X1-redirect", "ok", 1)
        do ..Obs("X1-redirect", "size", stream.Size)
    } catch e {
        do ..Obs("X1-redirect", "ok", 0)
        do ..Obs("X1-redirect", "error", $system.Status.GetErrorText(e.AsStatus()))
    }
}

}
