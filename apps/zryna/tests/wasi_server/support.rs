use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[path = "../fixtures/private_file.rs"]
pub(crate) mod common;
static NEXT: AtomicU64 = AtomicU64::new(0);
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub fn guard() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub struct Case {
    pub root: PathBuf,
    pub stem: String,
    pub bundle: PathBuf,
    pub configuration: common::Input,
    pub approval: common::Input,
}
impl Case {
    pub fn new(attempts: u16) -> Self {
        let config = json!({"listen":"127.0.0.1:0","attempts":attempts,"header_bytes":1024,
            "body_bytes":64,"request_ms":2000,"service_ms":5000});
        Self::from_config(&serde_json::to_vec(&config).expect("configuration"))
    }
    pub fn from_config(bytes: &[u8]) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("real workspace");
        let stem =
            format!("wasi-server-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let bundle = root.join(".zryna/out").join(format!("{stem}.wasi-server-run"));
        let configuration = common::Input::from_bytes(bytes);
        let approval = common::Input::from_bytes(
            &serde_json::to_vec(&json!({"schema":"zryna.wasi-server-listener-approval.v1",
            "configuration_sha256":format!("{:x}",Sha256::digest(bytes)),"allow_listen":true}))
            .expect("approval"),
        );
        Self { root, stem, bundle, configuration, approval }
    }
    pub fn command(&self, source: &str) -> Command {
        self.profile_command(source, "server-status-v1")
    }
    pub fn profile_command(&self, source: &str, profile: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_zryna"));
        command
            .args([
                "serve",
                source,
                "--profile",
                profile,
                "--export",
                "status",
                "--json",
                "--name",
                &self.stem,
                "--root",
            ])
            .arg(&self.root)
            .arg("--node")
            .arg(common::node())
            .arg("--server-config")
            .arg(&self.configuration.path)
            .arg("--listener-approval")
            .arg(&self.approval.path);
        command
    }
    pub fn manifest(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.bundle.join("zryna-wasi-server-manifest-v1.json"))
                .expect("complete server record"),
        )
        .expect("manifest")
    }
    pub fn spawn(&self, source: &str) -> Running {
        Running::start(self.command(source))
    }
    pub fn driver_request(&self, source: &str) -> zryna_driver::ServerRunRequest {
        zryna_driver::ServerRunRequest {
            workspace_root: self.root.clone(),
            entrypoint: source.into(),
            export: "status".into(),
            artifact_stem: self.stem.clone(),
            node_runtime: common::node(),
            configuration: self.configuration.path.clone(),
            listener_approval: self.approval.path.clone(),
        }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.bundle);
    }
}

pub struct Running {
    child: Child,
    receiver: mpsc::Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
    pub address: SocketAddr,
}
impl Running {
    pub fn start(mut command: Command) -> Self {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("actual public CLI");
        let mut output = BufReader::new(child.stdout.take().expect("stdout"));
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            output.read_line(&mut line).expect("readiness output");
            let _ = sender.send(line);
            let mut final_output = String::new();
            output.read_to_string(&mut final_output).expect("bounded final output");
            let _ = sender.send(final_output);
        });
        let mut running = Self {
            child,
            receiver,
            reader: Some(reader),
            address: "127.0.0.1:0".parse().expect("placeholder"),
        };
        let line =
            running.receiver.recv_timeout(Duration::from_secs(30)).expect("bounded readiness wait");
        let ready: Value = serde_json::from_str(&line).expect("ready JSON");
        assert_eq!(ready["kind"], "ready", "{ready}");
        running.address =
            ready["endpoint"].as_str().expect("actual endpoint").parse().expect("loopback socket");
        running
    }
    pub fn finish(&mut self) -> (ExitStatus, Value) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                break status;
            }
            assert!(Instant::now() < deadline, "service exceeded bounded test wait");
            std::thread::park_timeout(Duration::from_millis(5));
        };
        let output = self.receiver.recv_timeout(Duration::from_secs(1)).expect("final output");
        self.reader.take().expect("reader").join().expect("reader joined");
        let result = serde_json::from_str(output.trim()).expect("final JSON");
        assert!(TcpListener::bind(self.address).is_ok(), "actual listener closes after completion");
        (status, result)
    }
    pub fn kill(&mut self) {
        self.child.kill().expect("host termination");
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
pub fn exchange(address: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(1)).expect("actual local request");
    stream.set_read_timeout(Some(Duration::from_secs(3))).expect("read bound");
    stream.write_all(bytes).expect("request bytes");
    let mut output = Vec::new();
    match stream.read_to_end(&mut output) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        Err(error) => panic!("response: {error}"),
    }
    output
}
pub fn frame(address: SocketAddr, body: &[u8]) -> Vec<u8> {
    let mut bytes=format!("POST /local HTTP/1.1\r\nHost: {address}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
    bytes.extend(body);
    bytes
}
