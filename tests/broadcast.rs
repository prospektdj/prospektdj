use prospekt_dj::broadcast::{endpoint, Config};
#[test]
fn broadcast_validates_settings_and_never_serializes_password() {
    let mut config = Config::default();
    assert!(endpoint(&config).is_err());
    config.host = "localhost".into();
    config.password = "private@pass".into();
    assert!(endpoint(&config).is_ok());
    assert!(!serde_json::to_string(&config).unwrap().contains("private"));
    config.host = "localhost/invalid".into();
    assert!(endpoint(&config).is_err());
}
#[test]
fn local_shoutcast_receives_encoded_master_blocks() {
    if std::env::var_os("PROSPEKT_TEST_FFMPEG").is_none() {
        return;
    }
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut socket = loop {
            if let Ok((socket, _)) = listener.accept() {
                break socket;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "source did not connect"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut password = [0; 7];
        socket.read_exact(&mut password).unwrap();
        assert_eq!(&password, b"secret\n");
        socket.write_all(b"OK2\n").unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(2).any(|w| w == b"\n\n") {
            let mut b = [0];
            socket.read_exact(&mut b).unwrap();
            bytes.push(b[0]);
        }
        assert!(String::from_utf8_lossy(&bytes).contains("content-type:audio/mpeg"));
        let mut audio = [0; 512];
        socket.read_exact(&mut audio).unwrap();
        assert!(audio.iter().any(|b| *b != 0));
    });
    let config = Config {
        shoutcast: true,
        host: "127.0.0.1".into(),
        port,
        password: "secret".into(),
        ..Default::default()
    };
    let session = prospekt_dj::broadcast::start(&config, 44100).unwrap();
    for _ in 0..100 {
        let block = prospekt_dj::recording::Block {
            len: 512,
            frames: [[0.1, -0.1]; 512],
        };
        if session.sink.sender.send(block).is_err() {
            break;
        }
    }
    server.join().unwrap();
    drop(session);
}
#[test]
fn local_icecast_receives_authenticated_mp3_source() {
    if std::env::var_os("PROSPEKT_TEST_FFMPEG").is_none() {
        return;
    }
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut socket = loop {
            if let Ok((socket, _)) = listener.accept() {
                break socket;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            socket.read_exact(&mut b).unwrap();
            header.push(b[0]);
            assert!(header.len() < 16384);
        }
        let header = String::from_utf8_lossy(&header);
        assert!(header.starts_with("PUT /live.mp3"));
        assert!(header.contains("Authorization: Basic"));
        socket.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").unwrap();
        let mut data = [0; 512];
        socket.read_exact(&mut data).unwrap();
        assert!(data.iter().any(|b| *b != 0));
    });
    let config = Config {
        host: "127.0.0.1".into(),
        port,
        password: "secret".into(),
        ..Default::default()
    };
    let session = prospekt_dj::broadcast::start(&config, 44100).unwrap();
    for _ in 0..100 {
        if session
            .sink
            .sender
            .send(prospekt_dj::recording::Block {
                len: 512,
                frames: [[0.1, -0.1]; 512],
            })
            .is_err()
        {
            break;
        }
    }
    server.join().unwrap();
    drop(session);
}
#[test]
fn engine_streams_independently_of_recorder_and_handles_backpressure() {
    use prospekt_dj::{
        engine::{Command, Engine},
        recording::{CaptureSink, BLOCK_SIZE},
    };
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let (tx, rx) = crossbeam_channel::bounded(1);
    let overflow = Arc::new(AtomicBool::new(false));
    let mut engine = Engine::new(44100);
    engine.apply(Command::Broadcast(Some(CaptureSink {
        sender: tx,
        overflow: overflow.clone(),
    })));
    for _ in 0..BLOCK_SIZE {
        engine.render();
    }
    assert_eq!(rx.try_recv().unwrap().len, BLOCK_SIZE);
    engine.apply(Command::Recording(None));
    for _ in 0..(BLOCK_SIZE * 2) {
        engine.render();
    }
    assert!(overflow.load(Ordering::Relaxed));
    assert!(rx.try_recv().is_ok());
    engine.apply(Command::Broadcast(None));
    for _ in 0..BLOCK_SIZE {
        engine.render();
    }
    assert!(rx.try_recv().is_err());
}
