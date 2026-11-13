import sys, json, importlib.util
spec = importlib.util.spec_from_file_location("ev", sys.argv[1]); ev = importlib.util.module_from_spec(spec); spec.loader.exec_module(ev)
run = sys.argv[2]; cases = {c["id"]: c for c in json.load(open(sys.argv[3]))["cases"]}
obs, begun, ended, env = ev.parse(run + "/raw.log")
o = dict(obs["B1-C01"]); c = cases["B1-C01"]
alt = c["l1_permitted"]["modules_one_of"][0]
print("orig module_alt_ok", ev.module_alt_ok(o, alt))
o2 = dict(o); o2["modules.top.resources_loaded"] = "0"
print("resources_loaded=0 module_alt_ok", ev.module_alt_ok(o2, alt), "l1 bad:", ev.l1_compare(c, o2)[1])
print("slot_unchanged({}, top)", ev.slot_unchanged({}, "top"))
print("manifest_only w/o doc check:", ev.module_alt_ok({"modules.top.manifest_present":"1","modules.top.version":"2.0.0","modules.top.developer_mode":"1","modules.top.zpm_document_version":"9.9.9","modules.top.resources_loaded":"0","before.modules.top.resources_loaded":"0"}, {"top":{"manifest_only":"2.0.0","developer_mode":True,"resources":"as before"}}))
# repeated measurement overwrite
import tempfile, os; dp = os.path.join(tempfile.mkdtemp(), "dup.log"); open(dp,"w").write("CASE-BEGIN X\nOBS X status=OK\nOBS X status=error\nCASE-END X\n")
print("dup parse", ev.parse(dp)[0])
