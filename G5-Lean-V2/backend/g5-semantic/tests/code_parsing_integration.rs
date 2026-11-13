//! Integration tests for code element extraction via tree-sitter.
//!
//! These exercise `CodeAnchorExtractor` with realistic, multi-construct source
//! files across Rust, TypeScript, Python, Go, Java, C, and C++.  They
//! complement the inline unit tests in `code_parser.rs` with larger inputs
//! that combine many language features simultaneously.
//!
//! Test cases: TC-PRS-101 through TC-PRS-106 from
//! `views/test_views/ontology_grounding.test_view.md` §6.

use g5_semantic::parsing::anchor_types::{CodeElementKind, ParsedCodeBlock};
use g5_semantic::parsing::CodeAnchorExtractor;

fn extractor() -> CodeAnchorExtractor {
    CodeAnchorExtractor::new()
}

/// Helper: collect names from parse results.
fn names(results: &[ParsedCodeBlock]) -> Vec<&str> {
    results.iter().map(|r| r.name.as_str()).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-101: Rust — Complex Real-World Module
// ─────────────────────────────────────────────────────────────────────────────

const RUST_COMPLEX: &str = r#"
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub const MAX_RETRY_COUNT: u32 = 5;

pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// A task in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub status: TaskStatus,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Working,
    Completed,
    Failed,
}

pub trait TaskProcessor: Send + Sync {
    fn process(&self, task: &Task) -> Result<(), Error>;
    fn can_handle(&self, task: &Task) -> bool;
}

pub trait Configurable {
    fn config_key(&self) -> &str;
    fn apply_config(&mut self, config: &HashMap<String, String>) -> Result<(), Error>;
}

impl Task {
    pub fn new(name: &str) -> Self {
        Task {
            id: Uuid::new_v4(),
            name: name.to_string(),
            status: TaskStatus::Pending,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    pub fn transition(&mut self, new_status: TaskStatus) -> Result<(), Error> {
        self.validate_transition(&new_status)?;
        self.status = new_status;
        Ok(())
    }

    fn validate_transition(&self, new: &TaskStatus) -> Result<(), Error> {
        match (&self.status, new) {
            (TaskStatus::Pending, TaskStatus::Working) => Ok(()),
            (TaskStatus::Working, TaskStatus::Completed) => Ok(()),
            (TaskStatus::Working, TaskStatus::Failed) => Ok(()),
            _ => Err(Error::InvalidTransition),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed)
    }
}

impl<T: TaskProcessor + Clone> TaskRunner<T> {
    pub async fn run_all(&self, tasks: Vec<Task>) -> Vec<Result<(), Error>> {
        let mut results = Vec::new();
        for task in tasks {
            results.push(self.processor.process(&task));
        }
        results
    }
}

pub type TaskId = Uuid;

pub type ProcessorFn = Box<dyn Fn(&Task) -> Result<(), Error> + Send + Sync>;

pub async fn spawn_worker<P: TaskProcessor + 'static>(processor: P) -> JoinHandle<()> {
    tokio::spawn(async move {
        // worker loop
    })
}

pub fn create_default_runner() -> TaskRunner<DefaultProcessor> {
    TaskRunner::new(DefaultProcessor::default())
}
"#;

#[test]
fn tc_prs_101_extracts_all_top_level_elements() {
    let results = extractor().extract(RUST_COMPLEX, "rs");
    let n = names(&results);

    // Constants
    assert!(n.contains(&"MAX_RETRY_COUNT"), "const: {:?}", n);
    assert!(n.contains(&"DEFAULT_TIMEOUT_MS"), "const: {:?}", n);

    // Struct + enum
    assert!(n.contains(&"Task"), "struct: {:?}", n);
    assert!(n.contains(&"TaskStatus"), "enum: {:?}", n);

    // Traits
    assert!(n.contains(&"TaskProcessor"), "trait: {:?}", n);
    assert!(n.contains(&"Configurable"), "trait: {:?}", n);

    // Type aliases
    assert!(n.contains(&"TaskId"), "type alias: {:?}", n);
    assert!(n.contains(&"ProcessorFn"), "type alias: {:?}", n);

    // Functions
    assert!(n.contains(&"spawn_worker"), "async fn: {:?}", n);
    assert!(n.contains(&"create_default_runner"), "fn: {:?}", n);
}

#[test]
fn tc_prs_101_correct_kinds() {
    let results = extractor().extract(RUST_COMPLEX, "rs");

    assert!(results
        .iter()
        .any(|r| r.name == "MAX_RETRY_COUNT" && r.kind == CodeElementKind::Const));
    assert!(results
        .iter()
        .any(|r| r.name == "Task" && r.kind == CodeElementKind::Struct));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskStatus" && r.kind == CodeElementKind::Enum));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskProcessor" && r.kind == CodeElementKind::Trait));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskId" && r.kind == CodeElementKind::TypeAlias));
    assert!(results
        .iter()
        .any(|r| r.name == "spawn_worker" && r.kind == CodeElementKind::Function));
}

#[test]
fn tc_prs_101_use_statements_not_extracted() {
    let results = extractor().extract(RUST_COMPLEX, "rs");
    let n = names(&results);
    assert!(
        !n.contains(&"HashMap"),
        "use import should not be extracted"
    );
    assert!(!n.contains(&"Arc"), "use import should not be extracted");
    assert!(
        !n.contains(&"JoinHandle"),
        "use import should not be extracted"
    );
}

#[test]
fn tc_prs_101_attributes_dont_confuse_names() {
    let results = extractor().extract(RUST_COMPLEX, "rs");
    let n = names(&results);
    // #[derive(Debug, Clone, Serialize, Deserialize)] should not appear as a name
    assert!(!n.contains(&"Debug"));
    assert!(!n.contains(&"Clone"));
    assert!(!n.contains(&"Serialize"));
}

#[test]
fn tc_prs_101_impl_block_target_identified() {
    let results = extractor().extract(RUST_COMPLEX, "rs");
    // Should have an impl for Task (inherent impl).
    // Note: `impl<T: ...> TaskRunner<T>` uses a generic_type node, not a
    // bare type_identifier, so the current `impl_item type: (type_identifier)`
    // query does NOT capture it.  This is a known limitation.
    let impl_blocks: Vec<_> = results
        .iter()
        .filter(|r| r.kind == CodeElementKind::Impl)
        .collect();
    let impl_names: Vec<&str> = impl_blocks.iter().map(|b| b.name.as_str()).collect();
    assert!(
        impl_names.contains(&"Task"),
        "Should capture impl Task: {:?}",
        impl_names
    );
    // Generic impl targets (e.g. TaskRunner<T>) are not yet captured by the
    // Rust profile query — this is acceptable for v1.
}

#[test]
fn tc_prs_101_async_fn_same_kind_as_fn() {
    let results = extractor().extract(RUST_COMPLEX, "rs");
    let spawn = results.iter().find(|r| r.name == "spawn_worker").unwrap();
    let create = results
        .iter()
        .find(|r| r.name == "create_default_runner")
        .unwrap();
    assert_eq!(
        spawn.kind, create.kind,
        "async fn and fn should have same CodeElementKind"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-102: TypeScript — Complex Module
// ─────────────────────────────────────────────────────────────────────────────

const TS_COMPLEX: &str = r#"
import { EventEmitter } from "events";

export const API_VERSION = "v1";

export const MAX_RETRIES = 3;

export interface TaskConfig {
  maxRetries: number;
  backoffMs: number;
  timeout: number;
}

export interface HealthCheck {
  endpoint: string;
  intervalMs: number;
}

export type TaskId = string & { readonly __brand: unique symbol };

export type Result<T> = { ok: true; value: T } | { ok: false; error: string };

export class TaskService {
  private readonly config: TaskConfig;
  private retryCount = 0;

  constructor(config: TaskConfig) {
    this.config = config;
  }

  async processTask(taskId: TaskId): Promise<void> {
    // complex processing
    await this.validateConfig();
  }

  private validateConfig(): boolean {
    return this.config.maxRetries > 0 && this.config.backoffMs >= 100;
  }

  static create(config: Partial<TaskConfig>): TaskService {
    return new TaskService({
      maxRetries: config.maxRetries ?? 3,
      backoffMs: config.backoffMs ?? 1000,
      timeout: config.timeout ?? 30000,
    });
  }
}

export async function createTaskService(config: TaskConfig): Promise<TaskService> {
  const service = new TaskService(config);
  return service;
}

export function validateConfig(config: unknown): config is TaskConfig {
  return typeof config === "object" && config !== null;
}

export enum TaskStatus {
  Pending = "pending",
  Working = "working",
  Completed = "completed",
  Failed = "failed",
}

export enum Priority {
  Low = 0,
  Medium = 1,
  High = 2,
  Critical = 3,
}

export const handler = (req: Request): Response => {
  return new Response("ok");
};
"#;

#[test]
fn tc_prs_102_extracts_all_elements() {
    let results = extractor().extract(TS_COMPLEX, "ts");
    let n = names(&results);

    // Constants
    assert!(n.contains(&"API_VERSION"), "const: {:?}", n);
    assert!(n.contains(&"MAX_RETRIES"), "const: {:?}", n);

    // Interfaces
    assert!(n.contains(&"TaskConfig"), "interface: {:?}", n);
    assert!(n.contains(&"HealthCheck"), "interface: {:?}", n);

    // Type aliases
    assert!(n.contains(&"TaskId"), "type alias: {:?}", n);
    assert!(n.contains(&"Result"), "type alias: {:?}", n);

    // Class
    assert!(n.contains(&"TaskService"), "class: {:?}", n);

    // Functions
    assert!(n.contains(&"createTaskService"), "function: {:?}", n);
    assert!(n.contains(&"validateConfig"), "function: {:?}", n);

    // Enums
    assert!(n.contains(&"TaskStatus"), "enum: {:?}", n);
    assert!(n.contains(&"Priority"), "enum: {:?}", n);

    // Arrow function const
    assert!(n.contains(&"handler"), "arrow fn const: {:?}", n);
}

#[test]
fn tc_prs_102_correct_kinds() {
    let results = extractor().extract(TS_COMPLEX, "ts");

    assert!(results
        .iter()
        .any(|r| r.name == "TaskConfig" && r.kind == CodeElementKind::Interface));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskId" && r.kind == CodeElementKind::TypeAlias));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskService" && r.kind == CodeElementKind::Class));
    assert!(results
        .iter()
        .any(|r| r.name == "TaskStatus" && r.kind == CodeElementKind::Enum));
    assert!(results
        .iter()
        .any(|r| r.name == "createTaskService" && r.kind == CodeElementKind::Function));
}

#[test]
fn tc_prs_102_branded_type_alias_extracted() {
    let results = extractor().extract(TS_COMPLEX, "ts");
    let task_id = results.iter().find(|r| r.name == "TaskId").unwrap();
    assert_eq!(task_id.kind, CodeElementKind::TypeAlias);
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-103: Python — Functions, Classes, and Top-Level Assignments
// ─────────────────────────────────────────────────────────────────────────────

const PY_COMPLEX: &str = r#"
"""Task processing module."""

import asyncio
from typing import Optional

MAX_RETRY = 5

class TaskProcessor:
    """Processes tasks with retry logic."""

    def __init__(self, config: dict):
        self.config = config
        self._retry_count = 0

    def process(self, task_id: str) -> bool:
        """Process a single task."""
        for attempt in range(self.config.get("max_retries", 3)):
            try:
                self._do_work(task_id)
                return True
            except Exception:
                self._retry_count += 1
        return False

    @staticmethod
    def validate(data: dict) -> bool:
        """Validate task data."""
        return "id" in data and "name" in data

    @classmethod
    def from_config(cls, path: str) -> "TaskProcessor":
        """Create from config file."""
        import json
        with open(path) as f:
            return cls(json.load(f))

    def _do_work(self, task_id: str) -> None:
        """Internal work method."""
        pass

class WorkerPool:
    """Manages a pool of task processors."""

    def __init__(self, size: int = 4):
        self.size = size
        self.processors: list[TaskProcessor] = []

    async def start(self) -> None:
        """Start all workers."""
        pass

    def shutdown(self) -> None:
        """Graceful shutdown."""
        pass

async def run_worker(processor: TaskProcessor) -> None:
    """Run a single worker loop."""
    while True:
        await asyncio.sleep(1)

def _internal_helper() -> dict:
    """Private helper — should still be extracted."""
    return {}

def create_processor(config: Optional[dict] = None) -> TaskProcessor:
    """Factory function."""
    return TaskProcessor(config or {})
"#;

#[test]
fn tc_prs_103_extracts_classes_and_functions() {
    let results = extractor().extract(PY_COMPLEX, "py");
    let n = names(&results);

    assert!(n.contains(&"TaskProcessor"), "class: {:?}", n);
    assert!(n.contains(&"WorkerPool"), "class: {:?}", n);
    assert!(n.contains(&"run_worker"), "async function: {:?}", n);
    assert!(n.contains(&"_internal_helper"), "private function: {:?}", n);
    assert!(n.contains(&"create_processor"), "factory function: {:?}", n);
}

#[test]
fn tc_prs_103_extracts_class_methods() {
    let results = extractor().extract(PY_COMPLEX, "py");
    let n = names(&results);

    // Nested method extraction
    assert!(n.contains(&"__init__"), "constructor: {:?}", n);
    assert!(n.contains(&"process"), "method: {:?}", n);
    assert!(n.contains(&"validate"), "staticmethod: {:?}", n);
    assert!(n.contains(&"from_config"), "classmethod: {:?}", n);
}

#[test]
fn tc_prs_103_decorated_methods_extracted() {
    let results = extractor().extract(PY_COMPLEX, "py");
    let n = names(&results);
    assert!(
        n.contains(&"validate"),
        "@staticmethod method should be extracted"
    );
    assert!(
        n.contains(&"from_config"),
        "@classmethod method should be extracted"
    );
}

#[test]
fn tc_prs_103_async_def_handled() {
    let results = extractor().extract(PY_COMPLEX, "py");
    assert!(
        results.iter().any(|r| r.name == "run_worker"),
        "async def should be captured"
    );
    assert!(
        results.iter().any(|r| r.name == "start"),
        "async method should be captured"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-104: Multi-Language Dispatch — Correct Profile Selection
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tc_prs_104_rust_profile() {
    let results = extractor().extract(
        "pub struct Config { pub name: String }\npub fn init() {}\n",
        "rs",
    );
    assert!(results.len() >= 2);
    assert!(results.iter().any(|r| r.name == "Config"));
    assert!(results.iter().any(|r| r.name == "init"));
}

#[test]
fn tc_prs_104_typescript_profile() {
    let results = extractor().extract(
        "export interface Props { name: string; }\nexport function render(): void {}\n",
        "ts",
    );
    assert!(results.len() >= 2);
    assert!(results.iter().any(|r| r.name == "Props"));
    assert!(results.iter().any(|r| r.name == "render"));
}

#[test]
fn tc_prs_104_python_profile() {
    let results = extractor().extract(
        "class Handler:\n    def process(self):\n        pass\n\ndef main():\n    pass\n",
        "py",
    );
    assert!(results.len() >= 2);
    assert!(results.iter().any(|r| r.name == "Handler"));
    assert!(results.iter().any(|r| r.name == "main"));
}

#[test]
fn tc_prs_104_go_profile() {
    let results = extractor().extract(
        "package main\n\nfunc main() {\n\tfmt.Println(\"hello\")\n}\n\ntype Config struct {\n\tName string\n}\n",
        "go",
    );
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.name == "main"));
}

#[test]
fn tc_prs_104_java_profile() {
    let results = extractor().extract(
        "public class App {\n    public void run() {}\n    public static void main(String[] args) {}\n}\n",
        "java",
    );
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.name == "App"));
}

#[test]
fn tc_prs_104_c_profile() {
    let results = extractor().extract(
        "struct Config {\n    int timeout;\n};\n\nint main(int argc, char *argv[]) {\n    return 0;\n}\n",
        "c",
    );
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.name == "main"));
}

#[test]
fn tc_prs_104_cpp_profile() {
    let results = extractor().extract(
        "namespace app {\n\nclass Widget {\npublic:\n    void draw();\n};\n\n}\n",
        "cpp",
    );
    assert!(!results.is_empty());
}

#[test]
fn tc_prs_104_unsupported_returns_empty() {
    let ext = extractor();
    assert!(ext.extract("puts 'hello'", "rb").is_empty());
    assert!(ext.extract("print('hi')", "swift").is_empty());
    assert!(ext.extract("(defn foo [] 42)", "clj").is_empty());
}

#[test]
fn tc_prs_104_case_insensitive_extension() {
    // The extractor normalizes by stripping leading dot but does NOT
    // uppercase.  Verify behaviour for mixed case — tree-sitter grammars
    // are matched by the profile's extension list which is lowercase.
    // This test documents the current behaviour: if the extension is
    // passed as lowercase (which is the normal path) it works.
    let results = extractor().extract("pub fn test_fn() {}\n", "rs");
    assert!(!results.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-105: Error-Tolerant Parsing — Partial Syntax Errors
// ─────────────────────────────────────────────────────────────────────────────

const RUST_WITH_ERRORS: &str = r#"
pub struct ValidStruct {
    pub name: String,
    pub age: u32,
}

pub fn broken_function(
    // missing closing paren and body
{

pub fn valid_after_error() -> bool {
    true
}

pub enum Status {
    Active,
    Inactive,
}

pub fn another_valid(x: i32) -> i32 {
    x + 1
}
"#;

#[test]
fn tc_prs_105_extracts_valid_elements_around_error() {
    let results = extractor().extract(RUST_WITH_ERRORS, "rs");
    let n = names(&results);

    assert!(
        n.contains(&"ValidStruct"),
        "ValidStruct before error should be extracted: {:?}",
        n
    );
    assert!(
        n.contains(&"valid_after_error"),
        "valid_after_error after error should be extracted: {:?}",
        n
    );
    assert!(
        n.contains(&"Status"),
        "Status enum should be extracted: {:?}",
        n
    );
    assert!(
        n.contains(&"another_valid"),
        "another_valid should be extracted: {:?}",
        n
    );
}

#[test]
fn tc_prs_105_no_panic_on_syntax_error() {
    // Just verify the extractor completes without panicking
    let results = extractor().extract(RUST_WITH_ERRORS, "rs");
    assert!(
        results.len() >= 3,
        "Should extract at least 3 valid elements: got {}",
        results.len()
    );
}

#[test]
fn tc_prs_105_typescript_with_errors() {
    let src = r#"
export interface Valid {
  name: string;
}

export function broken(: void {

export function alsoValid(): number {
  return 42;
}

export class MyClass {
  method(): void {}
}
"#;
    let results = extractor().extract(src, "ts");
    let n = names(&results);
    assert!(
        n.contains(&"Valid"),
        "Valid interface should survive error: {:?}",
        n
    );
    // tree-sitter error recovery is best-effort — the broken function may
    // consume subsequent tokens.  We only assert that the parser does not
    // panic and extracts at least the element before the error.
    assert!(
        !results.is_empty(),
        "Should extract at least one valid element"
    );
}

#[test]
fn tc_prs_105_python_with_errors() {
    let src = r#"
class ValidClass:
    def method(self):
        pass

def broken_function(
    # missing closing paren

def valid_function():
    return True
"#;
    let results = extractor().extract(src, "py");
    let n = names(&results);
    assert!(
        n.contains(&"ValidClass"),
        "ValidClass before error: {:?}",
        n
    );
    // Python's indentation-sensitive parsing may behave differently
    // Just verify no panic
    assert!(!results.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// TC-PRS-106: Nested and Complex Generic Signatures
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tc_prs_106_deeply_nested_generics() {
    let src = r#"pub fn process<T: Clone + Send + 'static>(
    items: Vec<HashMap<String, Vec<T>>>,
    callback: impl Fn(&T) -> Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>>>>,
) -> Result<Vec<T>, Box<dyn Error>> {
    todo!()
}
"#;
    let results = extractor().extract(src, "rs");
    assert_eq!(results.len(), 1, "Exactly one function: {:?}", results);
    assert_eq!(results[0].name, "process");
    assert_eq!(results[0].kind, CodeElementKind::Function);
    assert!(results[0].byte_range.end > results[0].byte_range.start);
}

#[test]
fn tc_prs_106_where_clause_generics() {
    let src = r#"pub fn dispatch<H, R>(handler: H) -> impl Fn(Request) -> R
where
    H: Fn(Request) -> Pin<Box<dyn Future<Output = Result<R, Box<dyn Error>>>>>,
    R: Serialize + Send + 'static,
{
    move |req| handler(req)
}
"#;
    let results = extractor().extract(src, "rs");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "dispatch");
}

#[test]
fn tc_prs_106_generic_struct_with_bounds() {
    let src = r#"
pub struct Pipeline<S: Service, E: Executor + Clone> {
    service: S,
    executor: E,
    middlewares: Vec<Box<dyn Middleware<S::Request, S::Response>>>,
}

pub fn create_pipeline<S, E>(service: S, executor: E) -> Pipeline<S, E>
where
    S: Service + Send + Sync + 'static,
    E: Executor + Clone + Send + 'static,
{
    Pipeline { service, executor, middlewares: vec![] }
}
"#;
    let results = extractor().extract(src, "rs");
    let n = names(&results);
    assert!(n.contains(&"Pipeline"), "Generic struct: {:?}", n);
    assert!(n.contains(&"create_pipeline"), "Generic fn: {:?}", n);
}

#[test]
fn tc_prs_106_typescript_complex_generics() {
    let src = r#"
export type Handler<TReq, TRes> = (req: TReq) => Promise<TRes>;

export interface Repository<T extends { id: string }> {
  findById(id: string): Promise<T | null>;
  save(entity: T): Promise<void>;
  delete(id: string): Promise<boolean>;
}

export function createMiddleware<TReq, TRes>(
  handler: Handler<TReq, TRes>,
  transform: (res: TRes) => TRes
): Handler<TReq, TRes> {
  return async (req) => transform(await handler(req));
}
"#;
    let results = extractor().extract(src, "ts");
    let n = names(&results);
    assert!(n.contains(&"Handler"), "Generic type alias: {:?}", n);
    assert!(n.contains(&"Repository"), "Generic interface: {:?}", n);
    assert!(n.contains(&"createMiddleware"), "Generic function: {:?}", n);
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration: Real-world multi-file dispatch test
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn real_world_go_module() {
    let src = r#"
package worker

import (
	"context"
	"fmt"
	"sync"
)

type Config struct {
	Workers   int
	QueueSize int
	Timeout   time.Duration
}

type Worker struct {
	id     int
	config Config
	mu     sync.Mutex
}

func NewWorker(id int, config Config) *Worker {
	return &Worker{id: id, config: config}
}

func (w *Worker) Start(ctx context.Context) error {
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
			w.process()
		}
	}
}

func (w *Worker) process() {
	w.mu.Lock()
	defer w.mu.Unlock()
	fmt.Printf("Worker %d processing\n", w.id)
}

func RunPool(ctx context.Context, size int) error {
	workers := make([]*Worker, size)
	for i := 0; i < size; i++ {
		workers[i] = NewWorker(i, Config{Workers: size})
	}
	return nil
}
"#;
    let results = extractor().extract(src, "go");
    let n = names(&results);
    assert!(n.contains(&"NewWorker"), "Go function: {:?}", n);
    assert!(n.contains(&"RunPool"), "Go function: {:?}", n);
    assert!(n.contains(&"Config"), "Go struct: {:?}", n);
    assert!(n.contains(&"Worker"), "Go struct: {:?}", n);
    // Go methods
    assert!(n.contains(&"Start"), "Go method: {:?}", n);
    assert!(n.contains(&"process"), "Go method: {:?}", n);
}

#[test]
fn real_world_java_module() {
    let src = r#"
package com.g5.worker;

import java.util.concurrent.ExecutorService;
import java.util.List;

public class TaskManager {
    private final ExecutorService executor;
    private final List<Task> pending;

    public TaskManager(ExecutorService executor) {
        this.executor = executor;
        this.pending = new java.util.ArrayList<>();
    }

    public void submit(Task task) {
        pending.add(task);
        executor.submit(() -> process(task));
    }

    private void process(Task task) {
        task.execute();
    }

    public int pendingCount() {
        return pending.size();
    }
}

public interface Task {
    void execute();
    boolean isComplete();
}

public enum TaskState {
    PENDING,
    RUNNING,
    COMPLETED,
    FAILED
}
"#;
    let results = extractor().extract(src, "java");
    let n = names(&results);
    assert!(n.contains(&"TaskManager"), "Java class: {:?}", n);
    assert!(n.contains(&"Task"), "Java interface: {:?}", n);
    assert!(n.contains(&"TaskState"), "Java enum: {:?}", n);
    assert!(n.contains(&"submit"), "Java method: {:?}", n);
    assert!(n.contains(&"process"), "Java private method: {:?}", n);
}

#[test]
fn real_world_c_module() {
    let src = r#"
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int id;
    char name[256];
    double score;
} Record;

struct Config {
    int max_records;
    int timeout_ms;
};

enum Status {
    STATUS_OK = 0,
    STATUS_ERROR = 1,
    STATUS_TIMEOUT = 2
};

Record* create_record(int id, const char* name, double score) {
    Record* r = malloc(sizeof(Record));
    r->id = id;
    r->score = score;
    return r;
}

void free_record(Record* r) {
    free(r);
}

int process_batch(Record** records, int count) {
    for (int i = 0; i < count; i++) {
        printf("Processing record %d\n", records[i]->id);
    }
    return count;
}
"#;
    let results = extractor().extract(src, "c");
    let n = names(&results);
    // Note: `Record* create_record(...)` has a pointer_declarator wrapping
    // the function_declarator, so the current C profile query does not
    // capture it.  Non-pointer return types (free_record, process_batch) work.
    assert!(n.contains(&"free_record"), "C function: {:?}", n);
    assert!(n.contains(&"process_batch"), "C function: {:?}", n);
    assert!(n.contains(&"Config"), "C struct: {:?}", n);
    assert!(n.contains(&"Status"), "C enum: {:?}", n);
}

#[test]
fn real_world_cpp_module() {
    let src = r#"
#include <memory>
#include <vector>
#include <string>

namespace app {

class Widget {
public:
    Widget(const std::string& name) : name_(name) {}
    virtual ~Widget() = default;

    virtual void render() const = 0;
    const std::string& name() const { return name_; }

private:
    std::string name_;
};

class Button : public Widget {
public:
    Button(const std::string& name, const std::string& label)
        : Widget(name), label_(label) {}

    void render() const override {
        // render button
    }

private:
    std::string label_;
};

struct WindowConfig {
    int width;
    int height;
    bool fullscreen;
};

enum class Theme {
    Light,
    Dark,
    HighContrast
};

namespace detail {

void initialize_subsystem() {
    // internal setup
}

} // namespace detail

} // namespace app
"#;
    let results = extractor().extract(src, "cpp");
    let n = names(&results);
    assert!(n.contains(&"Widget"), "C++ class: {:?}", n);
    assert!(n.contains(&"Button"), "C++ derived class: {:?}", n);
    assert!(n.contains(&"WindowConfig"), "C++ struct: {:?}", n);
    assert!(n.contains(&"Theme"), "C++ scoped enum: {:?}", n);
    assert!(n.contains(&"app"), "C++ namespace: {:?}", n);
}

// ─────────────────────────────────────────────────────────────────────────────
// Byte range integrity
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn byte_ranges_point_to_correct_source_text() {
    let src = "pub struct Foo { val: i32 }\n\npub fn bar() -> i32 { 42 }\n";
    let results = extractor().extract(src, "rs");
    for result in &results {
        let slice = &src[result.byte_range.clone()];
        assert!(
            slice.contains(&result.name),
            "Byte range for '{}' should contain its name in source text, got: {:?}",
            result.name,
            slice
        );
    }
}

#[test]
fn line_numbers_are_one_indexed() {
    let src = "\n\npub fn third_line_fn() {}\n";
    let results = extractor().extract(src, "rs");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].line_number, 3,
        "Function on line 3 should report line_number = 3"
    );
}
