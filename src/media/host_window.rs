//! Скрытое top-level окно, к которому playwire привязывает SMTC (ADR-0024).

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, WM_APP, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::PCWSTR;

const CLASS_NAME: &str = "OrbitFM.SmtcHost";
const WINDOW_TITLE: &str = "OrbitFM SMTC";
const START_TIMEOUT: Duration = Duration::from_secs(5);
const WM_HOST_SHUTDOWN: u32 = WM_APP + 1;

/// Владелец скрытого окна и потока его сообщений.
///
/// Окно живёт столько же, сколько `HostWindow`; дескриптор не меняется между
/// показами и скрытиями окна eframe (ADR-0018, ADR-0024).
pub(super) struct HostWindow {
    hwnd: u64,
    thread: Option<JoinHandle<()>>,
}

impl HostWindow {
    pub(super) fn new() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("orbitfm-smtc".to_owned())
            .spawn(move || run(sender))
            .map_err(|err| format!("не удалось запустить поток окна SMTC: {err}"))?;

        match receiver.recv_timeout(START_TIMEOUT) {
            Ok(Ok(hwnd)) => Ok(Self {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(RecvTimeoutError::Timeout) => Err("окно SMTC не создано за 5 секунд".to_owned()),
            Err(RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                Err("поток окна SMTC завершился без результата".to_owned())
            }
        }
    }

    pub(super) fn hwnd(&self) -> u64 {
        self.hwnd
    }
}

impl Drop for HostWindow {
    fn drop(&mut self) {
        let hwnd = HWND(self.hwnd as usize as *mut core::ffi::c_void);
        // SAFETY: дескриптор принадлежит окну, созданному на потоке `thread`,
        // и остаётся валидным до обработки WM_HOST_SHUTDOWN.
        if let Err(err) =
            unsafe { PostMessageW(Some(hwnd), WM_HOST_SHUTDOWN, WPARAM(0), LPARAM(0)) }
        {
            tracing::warn!(%err, "не удалось отправить окну SMTC команду завершения");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(sender: mpsc::Sender<Result<u64, String>>) {
    let hwnd = match create_window() {
        Ok(hwnd) => hwnd,
        Err(err) => {
            let _ = sender.send(Err(err));
            return;
        }
    };
    let _ = sender.send(Ok(hwnd.0 as usize as u64));

    let mut message = MSG::default();
    // SAFETY: цикл сообщений обслуживает окна, созданные на этом потоке;
    // `message` — валидный буфер на всё время работы цикла.
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn create_window() -> Result<HWND, String> {
    // SAFETY: вызовы Win32 выполняются на потоке, который затем обслуживает
    // окно; `class_name` и `window_title` живут до конца функции, а `WNDCLASSW`
    // ссылается на них только во время `RegisterClassW`/`CreateWindowExW`.
    unsafe {
        let module =
            GetModuleHandleW(PCWSTR::null()).map_err(|err| format!("GetModuleHandleW: {err}"))?;
        let instance = HINSTANCE::from(module);

        let class_name = wide(CLASS_NAME);
        let window_title = wide(WINDOW_TITLE);
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..WNDCLASSW::default()
        };

        if RegisterClassW(&class) == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(ERROR_CLASS_ALREADY_EXISTS.0 as i32) {
                return Err(format!("RegisterClassW: {err}"));
            }
        }

        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_title.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(|err| format!("CreateWindowExW: {err}"))
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_HOST_SHUTDOWN {
        // SAFETY: сообщение приходит окну этого потока; DestroyWindow освобождает
        // его, PostQuitMessage завершает цикл GetMessageW в `run`.
        unsafe {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    // SAFETY: вся остальная обработка делегируется системной процедуре.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
