/// Unit tests for the parts of Docker Content Trust verification that decide from notary's output and manifests alone.
/// Test.PM.Integration.OrasContentTrust runs them against a real notary server.
Class Test.PM.Unit.OrasContentTrust Extends %UnitTest.TestCase
{

/// Returns a stream holding lines joined with LF, as notary writes them.
ClassMethod Output(lines... As %String) As %Stream.TmpCharacter
{
    set stream = ##class(%Stream.TmpCharacter).%New()
    for i = 1:1:$get(lines) {
        do stream.Write(lines(i) _ $char(10))
    }
    return stream
}

/// Pads a row of notary list output the way its tab writer does.
ClassMethod Row(
	tag As %String,
	hex As %String,
	size As %String,
	role As %String) As %String
{
    return tag _ "    " _ hex _ "    " _ size _ "            " _ role
}

Method TestParseTargets()
{
    set a = $translate($justify("", 64), " ", "a")
    set b = $translate($justify("", 64), " ", "b")
    set listing = ..Output(
        "NAME     DIGEST    SIZE (BYTES)    ROLE",
        "----     ------    ------------    ----",
        ..Row("1.0.0", a, 100, "targets/releases"),
        ..Row("1.0.0", b, 200, "targets"),
        ..Row("2.0.0", b, 200, "targets"),
        ..Row("3.0.0", a, 100, "targets/other"),
        ..Row("1.0.0_build__2026.1", a, 100, "targets"),
        ..Row("-bad", a, 100, "targets"),
        ..Row("4.0.0", "not-a-digest", 100, "targets"),
        ..Row("5.0.0", a, "big", "targets"))
    set targets = ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets(listing)
    do $$$AssertEquals(targets.%Get("1.0.0").%ToJSON(), {"digest": ("sha256:" _ a), "size": 100, "role": "targets/releases"}.%ToJSON(), "The first row for a tag wins")
    do $$$AssertEquals(targets.%Get("2.0.0").role, "targets")
    do $$$AssertEquals(targets.%Get("1.0.0_build__2026.1").size, 100, "Build and platform tags are tags")
    do $$$AssertNotTrue(targets.%IsDefined("3.0.0"), "Another delegation is not trusted")
    do $$$AssertNotTrue(targets.%IsDefined("-bad"), "A name that is not a tag is skipped")
    do $$$AssertNotTrue(targets.%IsDefined("4.0.0"), "A row without a sha256 digest is skipped")
    do $$$AssertNotTrue(targets.%IsDefined("5.0.0"), "A row without a size is skipped")
    do $$$AssertNotTrue(targets.%IsDefined("NAME") || targets.%IsDefined("----"), "Header lines are skipped")

    set empty = ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets(..Output("", "No targets present in this repository.", ""))
    do $$$AssertEquals(empty.%ToJSON(), "{}")
}

Method TestNotaryError()
{
    set stdout = ..Output("", "* fatal: client is offline")
    set stderr = ..Output("time=""x"" level=error msg=""could not reach http://notary:4443""")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(stdout, stderr, 1), "client is offline", "The fatal line on stdout is the reason")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(..Output(""), stderr, 1), "time=""x"" level=error msg=""could not reach http://notary:4443""", "Otherwise the last logged line")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(..Output(""), ..Output(""), 2), "notary exited with code 2")
}

/// A delegation could otherwise sign a name whose tabs and newline print as a row of a trusted role.
Method TestUnsafeTargetName()
{
    set metadata = ##class(%File).NormalizeDirectory(##class(%IPM.Utils.File).CreateTempDirectory() _ "metadata")
    do $$$AssertTrue(##class(%File).CreateDirectoryChain(metadata _ "targets"))
    do ..WriteTargets(metadata _ "targets.json", "1.0.0")
    do ..WriteTargets(metadata _ "targets/other.json", "2.0.0")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).UnsafeTargetName(metadata), "", "Tags are safe")

    do ..WriteTargets(metadata _ "targets/other.json", "9.9.9" _ $char(9) _ "abc" _ $char(10) _ "x")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).UnsafeTargetName(metadata), """9.9.9\tabc\nx""", "A name with a tab and newline is reported")
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(##class(%File).ParentDirectoryName(metadata))
}

ClassMethod WriteTargets(
	path As %String,
	name As %String)
{
    set targets = {}
    do targets.%Set(name, {"length": 1, "hashes": {"sha256": "AA=="}})
    set file = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(file.LinkToFile(path))
    do file.Clear()
    do file.Write({"signed": {"_type": "Targets", "targets": (targets)}}.%ToJSON())
    $$$ThrowOnError(file.%Save())
}

Method TestNormalizeTrustServer()
{
    do $$$AssertEquals(##class(%IPM.Repo.Oras.Definition).NormalizeTrustServer("HTTPS://Notary.Example.com:443/"), "https://notary.example.com")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.Definition).NormalizeTrustServer("http://notary:4443"), "http://notary:4443")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.Definition).NormalizeTrustServer(""), "")
    for refused = "ftp://notary:4443", "notary:4443", "http://" {
        try {
            do ##class(%IPM.Repo.Oras.Definition).NormalizeTrustServer(refused)
            do $$$AssertFailure("Accepted " _ refused)
        } catch e {
            do $$$AssertTrue(e.AsStatus() '= 1, "Refused " _ refused)
        }
    }
}

Method TestPackageLayer()
{
    set digest = "sha256:" _ $translate($justify("", 64), " ", "c")
    set package = {"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": (digest), "size": 391}
    set directory = {"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": (digest), "size": 10}
    set chosen = ##class(%IPM.Repo.Oras.PackageService).PackageLayer({"layers": [(directory), (package)]}, "r:1")
    do $$$AssertEquals(chosen.size, 391, "The package layer is chosen and other layers ignored")
    for manifest = {"layers": [(directory)]}, {"layers": [(package), (package)]}, {}, {"layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar", "digest": "md5:x", "size": 1}]} {
        try {
            do ##class(%IPM.Repo.Oras.PackageService).PackageLayer(manifest, "r:1")
            do $$$AssertFailure("Accepted " _ manifest.%ToJSON())
        } catch e {
            do $$$AssertTrue(e.AsStatus() '= 1, "Refused " _ manifest.%ToJSON())
        }
    }
}

}
