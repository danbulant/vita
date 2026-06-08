use std::char::decode_utf16;

use std::thread;
use std::time::Duration;

use vitasdk_sys::{
    sceImeDialogGetResult, sceImeDialogGetStatus, sceImeDialogInit, sceImeDialogTerm,
    SceImeDialogParam, SceImeDialogResult, PSP2_SDK_VERSION, SCE_COMMON_DIALOG_MAGIC_NUMBER,
    SCE_COMMON_DIALOG_STATUS_FINISHED, SCE_COMMON_DIALOG_STATUS_NONE,
    SCE_COMMON_DIALOG_STATUS_RUNNING, SCE_FALSE, SCE_IME_DIALOG_BUTTON_ENTER,
    SCE_IME_DIALOG_DIALOG_MODE_DEFAULT, SCE_IME_DIALOG_TEXTBOX_MODE_WITH_CLEAR,
    SCE_IME_ENTER_LABEL_SEARCH, SCE_IME_TYPE_DEFAULT,
};

use crate::plumbing::rendering::present_common_dialog;

const MAX_SEARCH_CHARS: usize = 128;

pub fn search_text(title: &str, initial_text: &str) -> Result<Option<String>, String> {
    let title = utf16_z(title, 128);
    let mut initial_text = utf16_z(initial_text, MAX_SEARCH_CHARS);
    let mut input_text = vec![0_u16; MAX_SEARCH_CHARS + 1];

    let mut param = unsafe { std::mem::zeroed::<SceImeDialogParam>() };
    param.sdkVersion = PSP2_SDK_VERSION;
    param.supportedLanguages = 0;
    param.languagesForced = SCE_FALSE as i32;
    param.type_ = SCE_IME_TYPE_DEFAULT;
    param.option = 0;
    param.dialogMode = SCE_IME_DIALOG_DIALOG_MODE_DEFAULT;
    param.textBoxMode = SCE_IME_DIALOG_TEXTBOX_MODE_WITH_CLEAR;
    param.title = title.as_ptr();
    param.maxTextLength = MAX_SEARCH_CHARS as u32;
    param.initialText = initial_text.as_mut_ptr();
    param.inputTextBuffer = input_text.as_mut_ptr();
    set_common_dialog_magic(&mut param);
    param.enterLabel = SCE_IME_ENTER_LABEL_SEARCH as u8;

    let init_result = unsafe { sceImeDialogInit(&param) };
    if init_result < 0 {
        let message = format!("IME init failed: 0x{:08x}", init_result as u32);
        eprintln!(
            "mpvrs: {message}, dialog_mode={}, textbox_mode={}, max_text_length={}, common_magic=0x{:08x}",
            param.dialogMode, param.textBoxMode, param.maxTextLength, param.commonParam.magic
        );
        return Err(message);
    }

    loop {
        match unsafe { sceImeDialogGetStatus() } {
            SCE_COMMON_DIALOG_STATUS_RUNNING => {
                present_common_dialog();
                thread::sleep(Duration::from_millis(16));
            }
            SCE_COMMON_DIALOG_STATUS_FINISHED => {
                let mut result = unsafe { std::mem::zeroed::<SceImeDialogResult>() };
                let result_code = unsafe { sceImeDialogGetResult(&mut result) };
                unsafe {
                    sceImeDialogTerm();
                }
                if result_code < 0 {
                    let message = format!("IME result failed: 0x{:08x}", result_code as u32);
                    eprintln!("mpvrs: {message}");
                    return Err(message);
                }
                if result.button == SCE_IME_DIALOG_BUTTON_ENTER as i32 {
                    return Ok(Some(utf16_z_to_string(&input_text)));
                }
                return Ok(None);
            }
            SCE_COMMON_DIALOG_STATUS_NONE => {
                unsafe {
                    sceImeDialogTerm();
                }
                return Ok(None);
            }
            _ => {
                present_common_dialog();
                thread::sleep(Duration::from_millis(16));
            }
        }
    }
}

fn set_common_dialog_magic(param: &mut SceImeDialogParam) {
    let common_param_addr = &param.commonParam as *const _ as usize as u32;
    param.commonParam.magic = SCE_COMMON_DIALOG_MAGIC_NUMBER.wrapping_add(common_param_addr);
}

fn utf16_z(text: &str, max_chars: usize) -> Vec<u16> {
    let mut out: Vec<u16> = text.encode_utf16().take(max_chars).collect();
    out.push(0);
    out
}

fn utf16_z_to_string(text: &[u16]) -> String {
    let len = text.iter().position(|ch| *ch == 0).unwrap_or(text.len());
    decode_utf16(text[..len].iter().copied())
        .map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect::<String>()
        .trim()
        .to_owned()
}
