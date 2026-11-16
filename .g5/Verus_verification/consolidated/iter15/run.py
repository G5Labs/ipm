#!/usr/bin/env python3
from pathlib import Path
import datetime,json,os,subprocess,sys,time
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[3];OUT=ROOT/'.g5/uplift/consolidated/iter15'
if '--no-native' in sys.argv:
 sys.exit(subprocess.call([sys.executable,str(HERE/'harness.py'),str(HERE),'--no-native']))
stamp=str(time.time_ns());network='ipm-iter15-'+stamp;registry=network+'-registry';image='ghcr.io/project-zot/zot-linux-arm64@sha256:14006bb4ca46e1bc250f3fc227b850fe1f7a9f5a751869af684f3b65cdb2c713';commands=[]
def run(cmd):
 p=subprocess.run(cmd,capture_output=True,text=True,timeout=90);commands.append({'command':cmd,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr});assert p.returncode==0,commands[-1];return p.stdout
OUT.mkdir(exist_ok=True)
try:
 run(['docker','network','create','--internal',network]);run(['docker','run','-d','--name',registry,'--network',network,'--network-alias','oras',image])
 env=dict(os.environ,IPM_NATIVE_NETWORK=network)
 code=subprocess.call([sys.executable,str(HERE/'harness.py'),str(HERE)]+(['--native-only'] if '--native-only' in sys.argv else []),env=env)
 logs=subprocess.run(['docker','logs',registry],capture_output=True,text=True)
 (OUT/('registry-'+stamp+'.log')).write_text(logs.stdout+logs.stderr)
 (OUT/('environment-'+stamp+'.json')).write_text(json.dumps({'registry_image':image,'commands':commands,'exit':code,'network':network,'exposed_host_ports':[]},indent=2)+'\n')
 sys.exit(code)
finally:
 subprocess.run(['docker','rm','-f',registry],capture_output=True);subprocess.run(['docker','network','rm',network],capture_output=True)
