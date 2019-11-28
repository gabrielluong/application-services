/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, scoped_keys::ScopedKey, scopes, FirefoxAccount};

impl FirefoxAccount {
    /// Migrate from a logged-in with a sessionToken Firefox Account.
    /// As part of this process the server duplicates
    /// a valid session into a new, independent session.
    ///
    /// * `session_token` - Hex-formatted session token.
    /// * `k_xcs` - Hex-formatted kXCS.
    /// * `k_sync` - Hex-formatted kSync.
    /// * `copy_session_token` - If true then the provided 'session_token' will be duplicated
    ///     and the resulting session will use a new session token. If false, the provided
    ///     token will be reused.
    ///
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn migrate_from_session_token(
        &mut self,
        session_token: &str,
        k_sync: &str,
        k_xcs: &str,
        copy_session_token: bool,
    ) -> Result<()> {
        // if there is already a session token on account, we error out.
        if self.state.session_token.is_some() {
            return Err(ErrorKind::IllegalState("Session Token is already set.").into());
        }

        let mut migration_session_token = session_token.to_string();

        if copy_session_token {
            let duplicate_session = self
                .client
                .duplicate_session(&self.state.config, &session_token)?;

            migration_session_token = duplicate_session.session_token;
        }
        // Trade our session token for a refresh token.

        let oauth_response = self.client.oauth_tokens_from_session_token(
            &self.state.config,
            &migration_session_token,
            &[scopes::PROFILE, scopes::OLD_SYNC],
        )?;
        self.handle_oauth_response(oauth_response, None)?;

        // Synthesize a scoped key from our kSync.
        let k_sync = hex::decode(k_sync)?;
        let k_sync = base64::encode_config(&k_sync, base64::URL_SAFE_NO_PAD);
        let k_xcs = hex::decode(k_xcs)?;
        let k_xcs = base64::encode_config(&k_xcs, base64::URL_SAFE_NO_PAD);
        let scoped_key_data = self.client.scoped_key_data(
            &self.state.config,
            &migration_session_token,
            scopes::OLD_SYNC,
        )?;
        let oldsync_key_data = scoped_key_data.get(scopes::OLD_SYNC).ok_or_else(|| {
            ErrorKind::IllegalState("The session token doesn't have access to kSync!")
        })?;
        let kid = format!("{}-{}", oldsync_key_data.key_rotation_timestamp, k_xcs);
        let k_sync_scoped_key = ScopedKey {
            kty: "oct".to_string(),
            scope: scopes::OLD_SYNC.to_string(),
            k: k_sync,
            kid,
        };
        self.state.session_token = Some(migration_session_token.clone());
        self.state
            .scoped_keys
            .insert(scopes::OLD_SYNC.to_string(), k_sync_scoped_key);
        Ok(())
    }
}
