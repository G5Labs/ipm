//! A real PostgreSQL wire transport that loses exactly one committed ACK.
//! Database behavior stays real; only delivery of CommandComplete/Ready changes.
use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_util::sync::CancellationToken;

pub struct CommitAckProxy {
    port: u16,
    upstream: Arc<Mutex<Option<(String, u16)>>>,
    armed: Arc<Mutex<Option<String>>>,
    drops: Arc<AtomicUsize>,
    changed: Arc<Notify>,
    cancel: CancellationToken,
    listener: tokio::task::JoinHandle<()>,
}
impl CommitAckProxy {
    pub async fn start() -> Arc<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let upstream = Arc::new(Mutex::new(None));
        let armed = Arc::new(Mutex::new(None));
        let drops = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(Notify::new());
        let cancel = CancellationToken::new();
        let task = {
            let upstream = upstream.clone();
            let armed = armed.clone();
            let drops = drops.clone();
            let changed = changed.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                loop {
                    let client = tokio::select! {_ = cancel.cancelled()=>break,client=listener.accept()=>match client{Ok((client,_))=>client,Err(_)=>break}};
                    let target = upstream
                        .lock()
                        .unwrap()
                        .clone()
                        .expect("tenant URL selected the upstream");
                    let armed = armed.clone();
                    let drops = drops.clone();
                    let changed = changed.clone();
                    let cancel = cancel.clone();
                    tokio::spawn(async move {
                        let _ = connection(client, target, armed, drops, changed, cancel).await;
                    });
                }
            })
        };
        Arc::new(Self {
            port,
            upstream,
            armed,
            drops,
            changed,
            cancel,
            listener: task,
        })
    }
    pub fn rewrite(&self, original: &str) -> String {
        let mut url = reqwest::Url::parse(original).unwrap();
        *self.upstream.lock().unwrap() = Some((
            url.host_str().unwrap().to_string(),
            url.port().unwrap_or(5432),
        ));
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(self.port)).unwrap();
        let parameters = url
            .query_pairs()
            .filter(|(key, _)| key != "sslmode")
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(parameters)
            .append_pair("sslmode", "disable");
        url.into()
    }
    pub fn arm(&self, sql: &str) {
        assert!(
            self.armed.lock().unwrap().replace(normalize(sql)).is_none(),
            "one loss at a time"
        );
    }
    pub fn drop_count(&self) -> usize {
        self.drops.load(Ordering::SeqCst)
    }
    pub async fn wait_for_drop(&self, prior: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let notification = self.changed.notified();
                if self.drop_count() > prior {
                    return;
                }
                notification.await;
            }
        })
        .await
        .expect("the selected transaction must commit and lose its ACK");
    }
}
impl Drop for CommitAckProxy {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.listener.abort();
    }
}
fn normalize(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
}

async fn frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<(u8, Vec<u8>)> {
    let tag = reader.read_u8().await?;
    let len = reader.read_u32().await? as usize;
    if !(4..=16 * 1024 * 1024).contains(&len) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid PostgreSQL frame",
        ));
    }
    let mut payload = vec![0; len - 4];
    reader.read_exact(&mut payload).await?;
    Ok((tag, payload))
}
async fn forward(
    writer: &mut (impl AsyncWrite + Unpin),
    tag: u8,
    payload: &[u8],
) -> io::Result<()> {
    writer.write_u8(tag).await?;
    writer.write_u32((payload.len() + 4) as u32).await?;
    writer.write_all(payload).await?;
    writer.flush().await
}
fn strings(payload: &[u8]) -> impl Iterator<Item = String> + '_ {
    payload
        .split(|byte| *byte == 0)
        .map(|value| String::from_utf8_lossy(value).into_owned())
}
async fn connection(
    client: TcpStream,
    target: (String, u16),
    armed: Arc<Mutex<Option<String>>>,
    drops: Arc<AtomicUsize>,
    changed: Arc<Notify>,
    cancel: CancellationToken,
) -> io::Result<()> {
    let server = TcpStream::connect(target).await?;
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    // sslmode=disable gives an ordinary startup frame, then tagged messages.
    let length = client_read.read_u32().await? as usize;
    if !(8..=65536).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid PostgreSQL startup",
        ));
    }
    let mut startup = vec![0; length - 4];
    client_read.read_exact(&mut startup).await?;
    server_write.write_u32(length as u32).await?;
    server_write.write_all(&startup).await?;
    server_write.flush().await?;
    let selected = Arc::new(AtomicBool::new(false));
    let requests = {
        let selected = selected.clone();
        async move {
            let mut statements = HashMap::<String, String>::new();
            let mut portals = HashMap::<String, String>::new();
            loop {
                let (tag, payload) = frame(&mut client_read).await?;
                let mut fields = strings(&payload);
                let sql = match tag {
                    b'P' => {
                        let name = fields.next().unwrap_or_default();
                        let sql = fields.next().unwrap_or_default();
                        statements.insert(name, sql);
                        None
                    }
                    b'B' => {
                        let portal = fields.next().unwrap_or_default();
                        let statement = fields.next().unwrap_or_default();
                        portals.insert(portal, statement);
                        None
                    }
                    b'E' => fields
                        .next()
                        .and_then(|portal| portals.get(&portal))
                        .and_then(|statement| statements.get(statement))
                        .cloned(),
                    b'Q' => fields.next(),
                    _ => None,
                };
                if let Some(sql) = sql {
                    let mut arm = armed.lock().unwrap();
                    if arm
                        .as_ref()
                        .is_some_and(|pattern| normalize(&sql).contains(pattern))
                    {
                        arm.take();
                        selected.store(true, Ordering::SeqCst);
                    }
                }
                forward(&mut server_write, tag, &payload).await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), io::Error>(())
        }
    };
    let responses = async move {
        loop {
            let (tag, payload) = frame(&mut server_read).await?;
            forward(&mut client_write, tag, &payload).await?;
            if tag == b'C' && payload == b"COMMIT\0" && selected.swap(false, Ordering::SeqCst) {
                // PostgreSQL has committed. CommandComplete reaches SQLx, but
                // the client loses ReadyForQuery and cannot confirm COMMIT.
                drops.fetch_add(1, Ordering::SeqCst);
                changed.notify_waiters();
                client_write.shutdown().await?;
                return Ok::<(), io::Error>(());
            }
            if tag == b'C' && payload == b"ROLLBACK\0" {
                selected.store(false, Ordering::SeqCst);
            }
        }
    };
    tokio::select! {result=requests=>result,result=responses=>result,_=cancel.cancelled()=>Ok(())}
}
