pub mod logger {
    use crossterm::{
        cursor, queue,
        style::{Color, Print, SetForegroundColor},
        terminal,
    };
    use std::{io::stdout, io::Write};

    /// A function to log an error message
    ///
    /// # Arguments
    /// * `message` - A String representation of an error to print
    pub fn error(message: String) {
        let mut stdout = stdout();
        queue!(
            stdout,
            cursor::MoveToNextLine(1),
            SetForegroundColor(Color::Red),
            Print(message),
            SetForegroundColor(Color::Reset),
            cursor::MoveToNextLine(1),
        )
        .unwrap();
        stdout.flush().expect("Could not flush on Stdout");
    }

    /// A function to log a warning message
    ///
    /// # Arguments
    /// * `message` - A String representation of an error to print
    pub fn warning(message: String) {
        let mut stdout = stdout();
        queue!(
            stdout,
            cursor::MoveToNextLine(1),
            SetForegroundColor(Color::DarkYellow),
            Print(message),
            SetForegroundColor(Color::Reset),
            cursor::MoveToNextLine(1),
        )
        .unwrap();
        stdout.flush().expect("Could not flush on Stdout");
    }

    pub fn clear() {
        let mut stdout = stdout();
        queue!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0),
        )
        .unwrap();
        stdout.flush().expect("Could not flush on Stdout");
    }

    /// A function to log an info message
    ///
    /// # Arguments
    /// * `message` - A String representation of an error to print
    pub fn info(message: String) {
        let mut stdout = stdout();
        queue!(
            stdout,
            SetForegroundColor(Color::Green),
            Print(message),
            cursor::MoveToNextLine(1),
        )
        .unwrap();
        stdout.flush().expect("Could not flush on Stdout");
    }

    /// A function to log a debug message if --debug is set
    ///
    /// # Arguments
    /// * `message` - A String representation of an error to print
    pub fn debug(message: String) {
        if std::env::args()
            .find(|x| x.to_owned() == String::from("--debug"))
            .is_some()
        {
            let mut stdout = stdout();
            queue!(
                stdout,
                SetForegroundColor(Color::DarkMagenta),
                Print(message),
                cursor::MoveToNextLine(1),
            )
            .unwrap();
            stdout.flush().expect("Could not flush on Stdout");
        }
    }
}
