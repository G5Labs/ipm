use super::*;

pub(crate) fn steer_params(
    d: &TestDispatcher,
    task: Uuid,
    input: Uuid,
    reservation: Uuid,
    content: &str,
) -> Value {
    let mut params = d.params(
        true,
        json!({
            "exec_id": d.task_exec_id(task, 1), "task_id":task, "attempt":1,
            "input_id":input, "reservation_id":reservation, "revision":1,
            "delivery_generation":1, "content":content,
        }),
    );
    params["context"]["idempotency_key"] = json!(format!("{input}:1"));
    params
}

// The pinned CLI recordings establish these native boundaries. This peer
// controls their timing to exercise the actual supervisor and dispatcher.
const PROVIDER: &str = r#"#!/usr/bin/env python3
import json, os, pathlib, select, sys, time
home = pathlib.Path(os.environ['HOME'])
def emit(frame): print(json.dumps(frame, separators=(',', ':')), flush=True)
assert '--replay-user-messages' not in sys.argv
initial = json.loads(sys.stdin.readline())
init = {'type':'system','subtype':'init','session_id':'22222222-2222-4222-8222-222222222222','model':'fixture-model','mcp_servers':[]}
result = {'type':'result','subtype':'success','is_error':False,'usage':{},'result':'done'}
emit(init)
emit({'type':'assistant','message':{'id':'before','role':'assistant','content':[{'type':'tool_use','id':'tool-1','name':'Bash','input':{'command':'controlled wait'}}]}})
# Do not even read the steered input until the test sees its flush receipt.
while not (home/'consume').exists(): time.sleep(.01)
message = json.loads(sys.stdin.readline())
(home/'received.json').write_text(json.dumps(message))
emit({'type':'user','message':{'role':'user','content':[{'type':'tool_result','tool_use_id':'tool-1','content':'finished'}]}})
if home.name == 'text':
    emit(result)
    # Claude can drain already queued input after stdin closes. The worker
    # must not hold EOF waiting for a replay that this provider never emits.
    assert select.select([sys.stdin], [], [], 5)[0], 'stdin held after turn result'
    assert sys.stdin.readline() == '', 'duplicate input'
    (home/'closed').touch()
    while not (home/'finish').exists(): time.sleep(.01)
    emit(init)
emit({'type':'assistant','message':{'id':'after','role':'assistant','content':[{'type':'text','text':message['message']['content']}]}})
emit(dict(result, result=message['message']['content']))
extra = sys.stdin.read()
(home/'extra.txt').write_text(extra)
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_steering_confirms_flush_before_provider_reads_without_replay_or_restart() {
    for mode in ["tool", "text", "stop"] {
        let providers = tempfile::tempdir().unwrap();
        let claude = executable_script(providers.path(), "claude-steering.py", PROVIDER);
        let mut d = TestDispatcher::spawn(
            bin(),
            DispatcherOptions {
                env: inherited_loader_env(),
                git_origin: true,
                ..Default::default()
            },
        )
        .await;
        stage_origin(
            &d.repo_root(),
            &d.repos[0].1,
            &d.origin.as_ref().unwrap().bare(d.repo),
        );
        let mut rpc = d.connect().await;
        let task = Uuid::new_v4();
        let exec = d.task_exec_id(task, 1);
        let mut policy: Value = serde_json::from_str(CLAUDE_PROMPT_POLICY).unwrap();
        policy["questions"] = json!("disabled");
        let prompt = "initial";
        let mut params = agent_turn_invoke(
            &d,
            task,
            1,
            "claude_code",
            "human",
            prompt,
            policy,
            "G5_CLAUDE_PATH",
            &claude,
        );
        let home = providers.path().join(mode);
        std::fs::create_dir(&home).unwrap();
        params["secrets"]["HOME"] = json!(home);
        params["input"]["provider"]["options"]["native_mid_turn_steering"] = json!(true);
        assert_eq!(
            result(rpc.call("modules.invoke", params).await)["disposition"],
            "started"
        );
        timeout(
            Duration::from_secs(15),
            rpc.module_event(&exec, "module.tool.started"),
        )
        .await
        .unwrap();
        let input = Uuid::new_v4();
        let reservation = Uuid::new_v4();
        let content = "  exact\nmessage \"quoted\" {{steered_prompt}}  ";
        let steer = steer_params(&d, task, input, reservation, content);
        let mut owner_call = steer.clone();
        owner_call["context"] = d.params(false, json!({}))["context"].clone();
        error(
            rpc.call("modules.input.steer", owner_call).await,
            "LIFECYCLE_AUTHORITY_REQUIRED",
        );
        assert_eq!(
            result(rpc.call("modules.input.steer", steer.clone()).await)["disposition"],
            "shipped"
        );
        let before = replay_events(&mut rpc, &d, &exec).await;
        assert_eq!(events_of(&before, "module.input.shipped").len(), 1);
        assert!(
            !home.join("received.json").exists(),
            "flush precedes provider read"
        );
        assert_eq!(
            result(rpc.call("modules.input.steer", steer.clone()).await)["disposition"],
            "shipped"
        );
        let mut conflict = steer.clone();
        conflict["content"] = json!("changed");
        error(
            rpc.call("modules.input.steer", conflict).await,
            "INPUT_CONFLICT",
        );
        if mode == "stop" {
            result(
                rpc.call(
                    "modules.cancel",
                    d.params(true, json!({"exec_id":exec,"reason":"owner_stop"})),
                )
                .await,
            );
            let outcome = timeout(Duration::from_secs(20), rpc.wait_outcome(&exec))
                .await
                .unwrap();
            assert_eq!(outcome["status"], "cancelled", "{outcome}");
            assert!(
                !home.join("received.json").exists(),
                "Stop may prevent reading flushed input"
            );
            let events = replay_events(&mut rpc, &d, &exec).await;
            assert_eq!(events_of(&events, "module.input.shipped").len(), 1);
            continue;
        }
        std::fs::write(home.join("consume"), "go").unwrap();
        if mode == "text" {
            timeout(Duration::from_secs(10), async {
                while !home.join("closed").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            // The process is alive but its input has closed. Atomic worker
            // admission must reject without writing; the API queues this text.
            let late = steer_params(&d, task, Uuid::new_v4(), Uuid::new_v4(), "after close");
            assert_eq!(
                result(rpc.call("modules.input.steer", late).await)["disposition"],
                "not_steerable"
            );
            std::fs::write(home.join("finish"), "go").unwrap();
        }
        let outcome = timeout(Duration::from_secs(25), rpc.wait_outcome(&exec))
            .await
            .unwrap();
        assert_eq!(outcome["result"]["status"], "succeeded", "{outcome}");
        let events = replay_events(&mut rpc, &d, &exec).await;
        assert!(events_of(&events, "module.input.consumed").is_empty());
        let tool = events
            .iter()
            .position(|e| e["kind"] == "module.tool.completed")
            .unwrap();
        let flush = events
            .iter()
            .position(|e| e["kind"] == "module.input.shipped")
            .unwrap();
        let answer = events
            .iter()
            .position(|e| e["kind"] == "module.message.final" && e["payload"]["content"] == content)
            .unwrap();
        assert!(flush < tool && tool < answer, "{events:?}");
        let received: Value =
            serde_json::from_slice(&std::fs::read(home.join("received.json")).unwrap()).unwrap();
        assert!(received.get("uuid").is_none());
        assert_eq!(received["message"]["content"], content);
        assert_eq!(std::fs::read_to_string(home.join("extra.txt")).unwrap(), "");
        assert_eq!(
            result(rpc.call("modules.input.steer", steer.clone()).await)["disposition"],
            "shipped"
        );
        let late = steer_params(&d, task, Uuid::new_v4(), Uuid::new_v4(), "late");
        assert_eq!(
            result(rpc.call("modules.input.steer", late).await)["disposition"],
            "not_steerable"
        );
        drop(rpc);
        d.restart().await;
        let mut rpc = d.connect().await;
        assert_eq!(
            result(rpc.call("modules.input.steer", steer.clone()).await)["disposition"],
            "shipped"
        );
        let mut conflict = steer;
        conflict["content"] = json!("changed after dispatcher restart");
        error(
            rpc.call("modules.input.steer", conflict).await,
            "INPUT_CONFLICT",
        );
    }
}
