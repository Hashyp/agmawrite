mod app;
mod cli;
mod comments;
mod document;
mod editing;
mod find;
mod help;
mod highlight;
mod input;
mod interactive_text;
mod preview;
mod theme;
mod ui;
mod watch;

/// Parses `args` and runs the agmawrite application.
pub fn run(args: impl IntoIterator<Item = String>) -> iced::Result {
    let args = match cli::parse_args(args.into_iter().collect()) {
        Ok(cli::ParseOutcome::Run(args)) => args,
        Ok(cli::ParseOutcome::Help) => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        Err(error) => {
            eprintln!("agmawrite: {error}\n\n{}", cli::USAGE);
            std::process::exit(1);
        }
    };

    iced::application(move || app::boot(&args), app::update, app::view)
        .title("agmawrite")
        .theme(app::theme)
        .subscription(app::subscription)
        .exit_on_close_request(false)
        .font(include_bytes!("../fonts/iAWriterMonoS-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Italic.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-Bold.ttf").as_slice())
        .font(include_bytes!("../fonts/iAWriterMonoS-BoldItalic.ttf").as_slice())
        .run()
}
