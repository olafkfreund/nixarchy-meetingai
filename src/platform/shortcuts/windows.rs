//! Windows hotkeys live on their own message-pump thread.
use super::{Action, Binding};
use std::sync::mpsc;
use std::time::Duration;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn virtual_key(key: &str) -> Option<u32> {
    match key {
        "backslash" => Some(VK_OEM_5 as u32),
        "space" => Some(VK_SPACE as u32),
        "enter" => Some(VK_RETURN as u32),
        key if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() => {
            Some(key.as_bytes()[0].to_ascii_uppercase() as u32)
        }
        key => key
            .strip_prefix('f')?
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=24).contains(n))
            .map(|n| VK_F1 as u32 + n - 1),
    }
}

pub(super) fn run(
    bindings: Vec<Binding>,
    events: mpsc::Sender<Action>,
    stop: mpsc::Receiver<()>,
    ready: mpsc::SyncSender<Result<Vec<String>, String>>,
) {
    let mut registered = Vec::new();
    let mut failed = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        let modifiers = MOD_NOREPEAT
            | if binding.ctrl { MOD_CONTROL } else { 0 }
            | if binding.shift { MOD_SHIFT } else { 0 }
            | if binding.alt { MOD_ALT } else { 0 };
        let id = index as i32 + 1;
        let success = virtual_key(&binding.key).is_some_and(|key| {
            // SAFETY: thread-owned hotkey registration, no window pointer.
            unsafe { RegisterHotKey(std::ptr::null_mut(), id, modifiers, key) != 0 }
        });
        if success {
            registered.push((id, binding.action));
        } else {
            failed.push(binding.display.clone());
        }
    }
    let _ = ready.send(Ok(failed));
    loop {
        if !matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            break;
        }
        let mut message = MSG::default();
        // SAFETY: message points to initialized storage, owned by this thread.
        while unsafe {
            PeekMessageW(
                &mut message,
                std::ptr::null_mut(),
                WM_HOTKEY,
                WM_HOTKEY,
                PM_REMOVE,
            )
        } != 0
        {
            if let Some((_, action)) = registered
                .iter()
                .find(|(id, _)| *id as usize == message.wParam)
            {
                let _ = events.send(*action);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for (id, _) in registered {
        // SAFETY: unregister on the thread that registered it.
        unsafe { UnregisterHotKey(std::ptr::null_mut(), id) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_default_key_has_a_windows_virtual_key() {
        for binding in Binding::defaults() {
            assert!(virtual_key(&binding.key).is_some());
        }
        assert_eq!(virtual_key("f24"), Some(VK_F24 as u32));
        assert_eq!(virtual_key("f25"), None);
    }
}
