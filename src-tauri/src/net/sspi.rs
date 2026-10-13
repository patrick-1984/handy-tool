//! Signing in to a proxy with NTLM or Negotiate through SSPI: the Windows
//! account by default (what browsers do), or a given user name and password.

use windows::Win32::Foundation::{SEC_E_OK, SEC_I_CONTINUE_NEEDED};
use windows::Win32::Security::Authentication::Identity::{
    AcquireCredentialsHandleW, DeleteSecurityContext, FreeContextBuffer, FreeCredentialsHandle,
    ISC_REQ_ALLOCATE_MEMORY, ISC_REQ_CONNECTION, InitializeSecurityContextW, SECBUFFER_TOKEN,
    SECBUFFER_VERSION, SECPKG_CRED_OUTBOUND, SECURITY_NATIVE_DREP, SecBuffer, SecBufferDesc,
};
use windows::Win32::Security::Credentials::SecHandle;
use windows::Win32::System::Rpc::{SEC_WINNT_AUTH_IDENTITY_UNICODE, SEC_WINNT_AUTH_IDENTITY_W};
use windows::core::HSTRING;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// One sign-in exchange (NTLM takes three messages, Kerberos usually one).
pub struct Context {
    credentials: SecHandle,
    context: Option<SecHandle>,
    target: Vec<u16>,
}

// SAFETY: the handles are only ever used by the thread that owns the Context.
unsafe impl Send for Context {}

impl Context {
    /// `package` = "Negotiate" or "NTLM"; `proxy_host` names the service
    /// (HTTP/proxy_host) for Kerberos. `credentials` = None: the Windows
    /// account Handy runs under.
    pub fn new(
        package: &str,
        proxy_host: &str,
        credentials: Option<(&str, &str)>,
    ) -> Result<Context, String> {
        // DOMAIN\user -> domain + user; user@domain stays whole (a UPN).
        let (mut user, mut domain, mut password) = (Vec::new(), Vec::new(), Vec::new());
        if let Some((name, secret)) = credentials {
            match name.split_once('\\') {
                Some((d, u)) => {
                    domain = wide(d);
                    user = wide(u);
                }
                None => user = wide(name),
            }
            password = wide(secret);
        }
        let identity = SEC_WINNT_AUTH_IDENTITY_W {
            User: user.as_mut_ptr(),
            UserLength: user.len() as u32,
            Domain: domain.as_mut_ptr(),
            DomainLength: domain.len() as u32,
            Password: password.as_mut_ptr(),
            PasswordLength: password.len() as u32,
            Flags: SEC_WINNT_AUTH_IDENTITY_UNICODE,
        };
        let auth_data = credentials.map(|_| &identity as *const _ as *const core::ffi::c_void);
        let mut handle = SecHandle::default();
        // SAFETY: every buffer outlives the call (SSPI copies the identity).
        let result = unsafe {
            AcquireCredentialsHandleW(
                None,
                &HSTRING::from(package),
                SECPKG_CRED_OUTBOUND,
                None,
                auth_data,
                None,
                None,
                &mut handle,
                None,
            )
        };
        // Wipe our copy of the password.
        password.iter_mut().for_each(|c| *c = 0);
        result.map_err(|error| format!("{package} sign-in is not available: {error}"))?;
        let mut target = wide(&format!("HTTP/{proxy_host}"));
        target.push(0);
        Ok(Context {
            credentials: handle,
            context: None,
            target,
        })
    }

    /// The next token to send, given the proxy's last challenge (None at the
    /// start). `done` = no further challenge is expected.
    pub fn step(&mut self, challenge: Option<&[u8]>) -> Result<(Vec<u8>, bool), String> {
        let mut input_buffer = SecBuffer {
            cbBuffer: challenge.map_or(0, |c| c.len() as u32),
            BufferType: SECBUFFER_TOKEN,
            pvBuffer: challenge.map_or(std::ptr::null_mut(), |c| c.as_ptr() as *mut _),
        };
        let input = SecBufferDesc {
            ulVersion: SECBUFFER_VERSION,
            cBuffers: 1,
            pBuffers: &mut input_buffer,
        };
        let mut output_buffer = SecBuffer {
            cbBuffer: 0,
            BufferType: SECBUFFER_TOKEN,
            pvBuffer: std::ptr::null_mut(),
        };
        let mut output = SecBufferDesc {
            ulVersion: SECBUFFER_VERSION,
            cBuffers: 1,
            pBuffers: &mut output_buffer,
        };
        let mut new_context = self.context.unwrap_or_default();
        let mut attributes = 0u32;
        // SAFETY: all buffers live across the call; SSPI allocates the output
        // token (ISC_REQ_ALLOCATE_MEMORY), freed below with FreeContextBuffer.
        let status = unsafe {
            InitializeSecurityContextW(
                Some(&self.credentials),
                self.context.as_ref().map(|c| c as *const SecHandle),
                Some(self.target.as_ptr()),
                ISC_REQ_ALLOCATE_MEMORY | ISC_REQ_CONNECTION,
                0,
                SECURITY_NATIVE_DREP,
                challenge.map(|_| &input as *const SecBufferDesc),
                0,
                Some(&mut new_context),
                Some(&mut output),
                &mut attributes,
                None,
            )
        };
        if status == SEC_E_OK || status == SEC_I_CONTINUE_NEEDED {
            self.context = Some(new_context);
        }
        let token = if output_buffer.pvBuffer.is_null() {
            Vec::new()
        } else {
            // SAFETY: SSPI allocated cbBuffer bytes; copy then free once.
            unsafe {
                let bytes = std::slice::from_raw_parts(
                    output_buffer.pvBuffer as *const u8,
                    output_buffer.cbBuffer as usize,
                )
                .to_vec();
                let _ = FreeContextBuffer(output_buffer.pvBuffer);
                bytes
            }
        };
        if status == SEC_E_OK {
            Ok((token, true))
        } else if status == SEC_I_CONTINUE_NEEDED {
            Ok((token, false))
        } else {
            Err(windows::core::Error::from(status).to_string())
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: handles created by this Context, released once.
        unsafe {
            if let Some(context) = self.context.take() {
                let _ = DeleteSecurityContext(&context);
            }
            let _ = FreeCredentialsHandle(&self.credentials);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_account_produces_a_first_token() {
        // No proxy needed: the first NTLM message is built locally.
        let mut context = Context::new("NTLM", "proxy.example", None).unwrap();
        let (token, done) = context.step(None).unwrap();
        assert!(!done);
        assert!(token.starts_with(b"NTLMSSP\0"));
    }

    #[test]
    fn given_credentials_produce_a_first_token() {
        let mut context =
            Context::new("NTLM", "proxy.example", Some(("CORP\\someone", "secret"))).unwrap();
        let (token, _) = context.step(None).unwrap();
        assert!(token.starts_with(b"NTLMSSP\0"));
    }
}
