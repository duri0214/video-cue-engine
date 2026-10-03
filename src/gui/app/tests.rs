use std::time::{Duration, Instant};

use eframe::{
    App,
    egui::{Event, FullOutput, PointerButton, Pos2, RawInput, Rect, epaint::Shape},
};

use super::*;

fn render(app: &mut BatchApp, context: &egui::Context, events: Vec<Event>) -> FullOutput {
    context.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 760.0))),
            events,
            ..Default::default()
        },
        |context| app.update(context, &mut eframe::Frame::_new_kittest()),
    )
}

fn text_position(output: &FullOutput, text: &str) -> Option<Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        Shape::Text(shape) if shape.galley.job.text == text => {
            Some(shape.pos + shape.galley.rect.center().to_vec2())
        }
        _ => None,
    })
}

fn click_start(app: &mut BatchApp, context: &egui::Context) {
    let output = render(app, context, Vec::new());
    let pos = text_position(&output, "一括解析を開始").expect("visible start button");
    for pressed in [true, false] {
        render(
            app,
            context,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(app.receiver.is_some(), "click starts a worker");
}

fn wait_for_worker(app: &mut BatchApp, context: &egui::Context) -> FullOutput {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let output = render(app, context, Vec::new());
        if app.receiver.is_none() {
            return output;
        }
        assert!(Instant::now() < deadline, "worker did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn user_can_retry_after_output_error_and_see_batch_results() {
    let temporary = tempfile::tempdir().unwrap();
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let missing = temporary.path().join("missing-downloads");
    let context = egui::Context::default();
    theme::install(&context);
    let mut app = BatchApp::new();
    app.output = Some(missing.clone());
    // Supply the native folder picker's response at the OS boundary.
    let (sender, receiver) = mpsc::channel();
    app.receiver = Some(receiver);
    sender
        .send(Message::Scanned(Some((
            input.clone(),
            BatchPlan::scan(&input),
        ))))
        .unwrap();
    drop(sender);
    let preview = render(&mut app, &context, Vec::new());
    assert!(text_position(&preview, "対象 2 件").is_some());

    click_start(&mut app, &context);
    wait_for_worker(&mut app, &context);

    assert!(
        app.error
            .as_ref()
            .unwrap()
            .contains("出力フォルダを使用できません")
    );
    assert!(!missing.exists());
    assert_eq!(app.succeeded + app.failed, 0);
    let (sender, receiver) = mpsc::channel();
    app.receiver = Some(receiver);
    sender
        .send(Message::OutputPicked(Some(temporary.path().to_owned())))
        .unwrap();
    drop(sender);
    render(&mut app, &context, Vec::new());

    click_start(&mut app, &context);
    let completed = wait_for_worker(&mut app, &context);

    assert!(text_position(&completed, "完了 2").is_some());
    assert!(text_position(&completed, "失敗 0").is_some());
    assert!(text_position(&completed, "今回の出力フォルダを開く").is_some());
    assert!(app.error.is_none());
    for row in &app.rows {
        assert!(matches!(row.status, VideoStatus::Succeeded));
        assert!(row.output.as_ref().unwrap().join("analysis.json").is_file());
    }
}
