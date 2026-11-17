Class Test.PM.Unit.OrasContentTrust Extends %UnitTest.TestCase
{

/// Output of "notary list <gun> -r targets/releases -r targets" for a collection with targets from several roles.
ClassMethod ListOutput() As %String
{
    set lf = $char(10)
    return "NAME           DIGEST                                                              SIZE (BYTES)    ROLE" _ lf
        _ "----           ------                                                              ------------    ----" _ lf
        _ "1.0.0          " _ ..Digest("a") _ "    1004            targets" _ lf
        _ "1.0.0_build.1  " _ ..Digest("b") _ "    988             targets/releases" _ lf
        _ "2.0.0          " _ ..Digest("c") _ "    1010            targets/qa" _ lf
        _ "3.0.0          not-a-digest                                                        1010            targets" _ lf
        _ "4.0.0          " _ ..Digest("d") _ "    12x             targets" _ lf
}

/// A sha256 digest in hex made of character.
ClassMethod Digest(character As %String) As %String
{
    return $translate($justify("", 64), " ", character)
}

Method TestParseTargets()
{
    do ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets(..ListOutput(), .targets)
    do $$$AssertEquals($listtostring($get(targets("1.0.0"))), "sha256:" _ ..Digest("a") _ ",1004,targets", "Target signed by the targets role")
    do $$$AssertEquals($listtostring($get(targets("1.0.0_build.1"))), "sha256:" _ ..Digest("b") _ ",988,targets/releases", "Target signed by targets/releases")
    do $$$AssertNotTrue($data(targets("2.0.0")), "Target signed only by another delegation is not trusted")
    do $$$AssertNotTrue($data(targets("3.0.0")), "Line without a sha256 digest is ignored")
    do $$$AssertNotTrue($data(targets("4.0.0")), "Line without a numeric size is ignored")
    do $$$AssertNotTrue($data(targets("NAME")), "Header is ignored")
}

Method TestParseTargetsWindowsLineEndings()
{
    do ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets($replace(..ListOutput(), $char(10), $char(13,10)), .targets)
    do $$$AssertEquals($listget($get(targets("1.0.0")), 2), 1004)
}

Method TestParseTargetsEmptyCollection()
{
    set targets("stale") = ""
    do ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets($char(10) _ "No targets present in this repository." _ $char(10), .targets)
    do $$$AssertNotTrue($data(targets), "No targets, and none left from before")
}

/// The client lists each name once, so a name listed twice means rows that are not the client's own.
Method TestParseTargetsDuplicateName()
{
    set text = ..ListOutput() _ "1.0.0          " _ ..Digest("e") _ "    7               targets" _ $char(10)
    do ##class(%IPM.Repo.Oras.ContentTrust).ParseTargets(text, .targets)
    do $$$AssertNotTrue($data(targets("1.0.0")), "A name listed twice is not trusted")
    do $$$AssertTrue($data(targets("1.0.0_build.1")), "Other names are unaffected")
}

/// Rows count only where the metadata files, chained by hash from timestamp.json, sign the same target in the same role.
Method TestSignedTargetsAgreeWithMetadata()
{
    set directory = ..WriteMetadata(.files)
    set lf = $char(10)
    set output = "1.0.0    " _ ..Digest("b") _ "    988     targets/releases" _ lf
        _ "1.1.0    " _ ..Digest("d") _ "    1100    targets/releases" _ lf
        _ "2.0.0    " _ ..Digest("c") _ "    1010    targets" _ lf
        _ "0.9.0    " _ ..Digest("e") _ "    7       targets" _ lf
        _ "3.0.0    " _ ..Digest("f") _ "    5       targets" _ lf
    do ##class(%IPM.Repo.Oras.ContentTrust).SignedTargets(output, directory, .targets)
    do $$$AssertEquals($listtostring($get(targets("1.0.0"))), "sha256:" _ ..Digest("b") _ ",988", "targets/releases takes precedence")
    do $$$AssertEquals($listtostring($get(targets("2.0.0"))), "sha256:" _ ..Digest("c") _ ",1010", "Name outside the releases paths comes from targets")
    do $$$AssertNotTrue($data(targets("1.1.0")), "Row whose role differs from the metadata is not trusted")
    do $$$AssertNotTrue($data(targets("0.9.0")), "Row whose digest differs from the metadata is not trusted")
    do $$$AssertNotTrue($data(targets("3.0.0")), "Row for a name the metadata does not sign is not trusted")

    // A releases file left from an earlier verification does not match the snapshot, so only targets counts
    do ..WriteFile(directory _ "targets/releases.json", $replace(files("targets/releases"), "988", "989"))
    do ##class(%IPM.Repo.Oras.ContentTrust).ReadTargets(directory, .signed)
    do $$$AssertEquals($listtostring($get(signed("1.0.0"))), "sha256:" _ ..Digest("a") _ ",1004,targets", "Stale releases file is ignored")

    do ..WriteFile(directory _ "targets.json", $replace(files("targets"), "1004", "1005"))
    try {
        do ##class(%IPM.Repo.Oras.ContentTrust).ReadTargets(directory, .signed)
        do $$$AssertTrue(0, "Targets metadata that does not match the snapshot is refused")
    } catch e {
        do $$$AssertTrue(e.DisplayString() [ "no verified targets metadata", e.DisplayString())
    }
    do ##class(%IPM.Utils.File).RemoveDirectoryTree(directory)
}

/// Writes metadata files to a new directory and returns it, with each file's content in files(role).
/// targets signs 0.9.0, 1.0.0, 1.1.0 and 2.0.0, and delegates names starting "1.0" to targets/releases,
/// which signs 1.0.0 and 2.0.0.
ClassMethod WriteMetadata(Output files) As %String
{
    set directory = ##class(%IPM.Utils.File).CreateTempDirectory()
    set targets = {"0.9.0": (..Target("a", 7)), "1.0.0": (..Target("a", 1004)), "1.1.0": (..Target("d", 1100)), "2.0.0": (..Target("c", 1010))}
    set releasesDelegation = {"name": "targets/releases", "keyids": [], "threshold": 1, "paths": ["1.0"]}
    set files("targets") = {"signed": {"_type": "Targets", "delegations": {"keys": {}, "roles": [ (releasesDelegation) ]}, "targets": (targets), "version": 2}, "signatures": []}.%ToJSON()
    set releases = {"1.0.0": (..Target("b", 988)), "2.0.0": (..Target("e", 7))}
    set files("targets/releases") = {"signed": {"_type": "Targets", "targets": (releases), "version": 1}, "signatures": []}.%ToJSON()
    set meta = {"targets": (..Meta(files("targets"))), "targets/releases": (..Meta(files("targets/releases")))}
    set files("snapshot") = {"signed": {"_type": "Snapshot", "meta": (meta), "version": 3}, "signatures": []}.%ToJSON()
    set files("timestamp") = {"signed": {"_type": "Timestamp", "meta": {"snapshot": (..Meta(files("snapshot")))}, "version": 4}, "signatures": []}.%ToJSON()
    do ##class(%IPM.Utils.File).CreateDirectoryChain(directory _ "targets/")
    set role = ""
    for {
        set role = $order(files(role), 1, content)
        quit:role=""
        do ..WriteFile(directory _ role _ ".json", content)
    }
    return directory
}

/// Target metadata with a sha256 digest made of character.
ClassMethod Target(
	character As %String,
	length As %Integer) As %DynamicObject
{
    return {"hashes": {"sha256": ($system.Encryption.Base64Encode(##class(%xsd.hexBinary).XSDToLogical(..Digest(character)), 1))}, "length": (length)}
}

/// Metadata that a parent role gives for a file with the given content.
ClassMethod Meta(content As %String) As %DynamicObject
{
    return {"hashes": {"sha256": ($system.Encryption.Base64Encode($system.Encryption.SHAHash(256, content), 1))}, "length": ($length(content))}
}

ClassMethod WriteFile(
	filename As %String,
	content As %String)
{
    set file = ##class(%Stream.FileBinary).%New()
    do file.LinkToFile(filename)
    do file.Write(content)
    do file.%Save()
}

/// Trust data from a plain http server could be replaced in transit before the first verification pins its root.
Method TestHttpTrustServerRefused()
{
    set trust = ##class(%IPM.Repo.Oras.ContentTrust).%New("http://notary.example.com")
    try {
        do trust.GetTarget("registry.example.com/ipm/module", "1.0.0")
        do $$$AssertTrue(0, "An http trust server is refused")
    } catch e {
        do $$$AssertTrue(e.DisplayString() [ "must use https", e.DisplayString())
    }
}

Method TestNotaryErrorUsesFatalMessage()
{
    set text = $char(10) _ "* fatal: client is offline" _ $char(10,10)
        _ "time=""2026-11-17T16:51:32Z"" level=error msg=""could not reach http://notary:9999""" _ $char(10)
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(text), "client is offline")
}

Method TestNotaryErrorFallsBackToLastLine()
{
    set text = "first" _ $char(13,10) _ "  last" _ $char(9) _ "line  " _ $char(13,10,10)
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(text), "last line")
    do $$$AssertEquals(##class(%IPM.Repo.Oras.ContentTrust).NotaryError(""), "")
}

/// Registry content is passed on only when it has the signed size and digest.
Method TestReadVerified()
{
    set content = "module archive bytes"
    set digest = "sha256:" _ $zconvert(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.SHAHash(256, content)), "L")
    do $$$AssertEquals(..ReadVerified(content, digest, $length(content)), "ok:" _ content)
    set other = $translate(content, "m", "M")
    do $$$AssertEquals(..ReadVerified(other, digest, $length(content)), "Test content from the registry does not match its signed digest " _ digest _ ".")
    do $$$AssertEquals(..ReadVerified(content _ "x", digest, $length(content)), "Test content from the registry is larger than its signed size of " _ $length(content) _ " bytes.")
    do $$$AssertEquals(..ReadVerified($extract(content, 2, *), digest, $length(content)), "Test content from the registry is " _ ($length(content) - 1) _ " bytes, but its signed size is " _ $length(content) _ " bytes.")
    do $$$AssertEquals(..ReadVerified(content, "sha512:" _ ..Digest("a"), $length(content)), "Test content has digest sha512:" _ ..Digest("a") _ ", which is not a sha256 digest.")
}

/// Passes content, in small chunks as a streamed response would, to the oras helper's read_verified.
/// Returns "ok:" and what it passed on, or the message it raised.
ClassMethod ReadVerified(
	content As %String,
	digest As %String,
	size As %Integer) As %String [ Language = python ]
{
    import iris

    helper = iris.cls("%IPM.Repo.Oras.PackageService").OrasHelper()
    data = content.encode("latin-1")

    class Response:
        def iter_content(self, chunk_size):
            for start in range(0, len(data), 3):
                yield data[start:start + 3]

    received = bytearray()
    try:
        helper.read_verified(Response(), digest, size, "Test content", received.extend)
    except helper.ContentMismatchError as ex:
        return str(ex)
    return "ok:" + bytes(received).decode("latin-1")
}

/// The package service verifies against the repo's trust server, and publishing never does.
Method TestServicesGetTrustServer()
{
    set def = ##class(%IPM.Repo.Oras.Definition).%New()
    set def.URL = "https://registry.example.com"
    do $$$AssertEquals(def.GetPackageService().TrustServer, "", "No trust server by default")
    set def.TrustServer = "https://notary.example.com"
    do $$$AssertEquals(def.GetPackageService().TrustServer, "https://notary.example.com")
    do $$$AssertEquals(def.GetPublishService().TrustServer, "")
}

}
