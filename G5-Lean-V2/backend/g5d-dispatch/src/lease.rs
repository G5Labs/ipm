//! Token claims are checked after receipt from the authenticated host. JWT
//! signature verification and authoritative lease fencing belong to that host
//! and the control plane, never to this untrusted workspace.
use crate::protocol::now_ms;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const MAX_OUTSTANDING_RENEWALS: usize = 128;

#[derive(Deserialize)]
struct Claims {
    vm_id: uuid::Uuid,
    exp: u64,
}
pub struct Lease {
    token: String,
    vm_id: uuid::Uuid,
    expires: Instant,
    renew_at: Instant,
    pub seq: u64,
    pub installed_seq: u64,
    pending: BTreeMap<u64, Instant>,
}
impl Lease {
    pub fn new(token: String, vm_id: uuid::Uuid, ttl: u64) -> anyhow::Result<Self> {
        let bound = Self::bound(&token, vm_id, ttl)?;
        let now = Instant::now();
        Ok(Self {
            token,
            vm_id,
            expires: now + bound,
            // Preparation consumed part of the control plane's initial lease.
            // Ask for an authoritative grant as soon as the first control
            // connection is ready; sending the request extends nothing locally.
            renew_at: now,
            seq: 0,
            installed_seq: 0,
            pending: BTreeMap::new(),
        })
    }
    fn bound(token: &str, vm_id: uuid::Uuid, ttl: u64) -> anyhow::Result<Duration> {
        anyhow::ensure!(
            token.len() <= 16384 && ttl > 0 && ttl <= 86400,
            "invalid workspace lease"
        );
        let parts = token.split('.').collect::<Vec<_>>();
        anyhow::ensure!(parts.len() == 3, "invalid workspace token");
        let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1])?)?;
        anyhow::ensure!(claims.vm_id == vm_id, "workspace token vm_id mismatch");
        let remaining = claims.exp.saturating_mul(1000).saturating_sub(now_ms());
        anyhow::ensure!(remaining > 0, "expired workspace token");
        Ok(Duration::from_millis(remaining).min(Duration::from_secs(ttl)))
    }
    pub fn valid(&self) -> bool {
        Instant::now() < self.expires
    }
    pub fn token(&self) -> &str {
        &self.token
    }
    /// Call only after the previous authenticated peer has stopped processing
    /// requests and replies. Its outstanding requests can no longer complete;
    /// the replacement peer needs a fresh request without gaining any custody.
    /// The first connection also renews immediately; accepted grants resume
    /// the normal half-TTL schedule in `install_with_publish`.
    pub fn peer_replaced(&mut self) {
        self.pending.clear();
        let now = Instant::now();
        if now < self.expires {
            self.renew_at = now;
        }
    }
    pub fn due(&mut self) -> Option<u64> {
        self.due_at(Instant::now())
    }
    fn due_at(&mut self, now: Instant) -> Option<u64> {
        if now >= self.expires || now < self.renew_at {
            return None;
        }
        // A missing reply is retried, with a new sequence, without extending custody.
        self.renew_at = now + Duration::from_secs(1).min((self.expires - now) / 2);
        // Bound memory without evicting an outstanding slow response. Once the
        // window is full, a reply must settle existing requests before we send
        // more; merely retrying never renews custody.
        if self.pending.len() >= MAX_OUTSTANDING_RENEWALS {
            return None;
        }
        self.seq = self.seq.checked_add(1)?;
        self.pending.insert(self.seq, now);
        Some(self.seq)
    }
    pub fn install(&mut self, seq: u64, token: String, ttl: u64) -> anyhow::Result<()> {
        self.install_with_publish(seq, token, ttl, |_| Ok(()))
    }

    /// Publish the validated credential before advancing custody. A failed
    /// publication cannot extend the lease while children retain an old token.
    pub fn install_with_publish(
        &mut self,
        seq: u64,
        token: String,
        ttl: u64,
        publish: impl FnOnce(&str) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            seq > self.installed_seq && self.valid(),
            "obsolete lease reply"
        );
        let sent = *self
            .pending
            .get(&seq)
            .ok_or_else(|| anyhow::anyhow!("unsolicited lease reply"))?;
        let bound = Self::bound(&token, self.vm_id, ttl)?;
        let now = Instant::now();
        anyhow::ensure!(now < self.expires, "expired current lease");
        // A delayed response cannot create a fresh full TTL at arrival.
        let expires = (sent + Duration::from_secs(ttl)).min(now + bound);
        anyhow::ensure!(expires > now, "expired lease reply");
        publish(&token)?;
        self.token = token;
        self.installed_seq = seq;
        self.expires = expires;
        self.renew_at = now + (expires - now) / 2;
        // Newer requests remain eligible. Older replies and duplicates may no
        // longer roll back the token or deadline established by this response.
        self.pending.retain(|pending, _| *pending > seq);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn token(vm_id: uuid::Uuid) -> String {
        token_with_expiry(vm_id, now_ms() / 1000 + 60)
    }
    fn token_with_expiry(vm_id: uuid::Uuid, exp: u64) -> String {
        format!(
            "e30.{}.sig",
            URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&serde_json::json!({"vm_id":vm_id,"exp":exp})).unwrap())
        )
    }
    #[test]
    fn renewal_publication_failure_preserves_custody_and_can_retry() {
        let vm = uuid::Uuid::new_v4();
        let old = token(vm);
        let new = token_with_expiry(vm, now_ms() / 1000 + 120);
        let mut lease = Lease::new(old.clone(), vm, 60).unwrap();
        lease.pending.insert(1, Instant::now());
        let before = lease.expires;
        assert!(lease
            .install_with_publish(1, new.clone(), 60, |_| anyhow::bail!("disk unavailable"))
            .is_err());
        assert_eq!(lease.token(), old);
        assert_eq!(lease.expires, before);
        assert_eq!(lease.installed_seq, 0);
        lease
            .install_with_publish(1, new.clone(), 60, |published| {
                assert_eq!(published, new);
                Ok(())
            })
            .unwrap();
        assert_eq!(lease.token(), new);
        assert!(lease
            .install_with_publish(1, old, 60, |_| panic!("stale reply published"))
            .is_err());
        lease.pending.insert(2, Instant::now());
        assert!(lease
            .install_with_publish(2, token(uuid::Uuid::new_v4()), 60, |_| panic!(
                "wrong VM published"
            ))
            .is_err());
        assert_eq!(lease.token(), new);
    }

    #[test]
    fn first_control_connection_renews_immediately_without_extending_custody() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        let expires = lease.expires;
        let seq = lease
            .due()
            .expect("first connection renews without a half-TTL delay");
        assert_eq!(seq, 1);
        assert_eq!(lease.installed_seq, 0);
        assert_eq!(lease.expires, expires);
        lease.install(seq, token(vm), 60).unwrap();
        assert_eq!(lease.installed_seq, 1);
        assert!(
            lease.due().is_none(),
            "an accepted grant restores half-TTL scheduling"
        );
    }

    #[test]
    fn renew_requires_current_request_matching_vm_and_valid_ttl() {
        let vm = uuid::Uuid::new_v4();
        let old = token(vm);
        let mut lease = Lease::new(old.clone(), vm, 60).unwrap();
        lease.pending.insert(1, Instant::now());
        assert!(lease.install(1, token(uuid::Uuid::new_v4()), 60).is_err());
        assert_eq!(lease.token(), old);
        assert!(lease.install(0, token(vm), 60).is_err());
        assert!(lease.install(1, token(vm), 0).is_err());
        assert!(lease
            .install(1, token_with_expiry(vm, now_ms() / 1000 - 1), 60)
            .is_err());
        assert_eq!(lease.installed_seq, 0);
        assert_eq!(lease.token(), old);
        lease.install(1, token(vm), 60).unwrap();
        assert!(lease.install(1, token(vm), 60).is_err());
        lease
            .pending
            .insert(2, Instant::now() - Duration::from_secs(2));
        assert!(lease.install(2, token(vm), 1).is_err());
    }

    #[test]
    fn delayed_reply_after_a_retry_installs_without_restarting_its_ttl() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        // Model a 1.5 second response while the second request has already
        // departed. These are the same scheduler and install boundaries used
        // by the RPC loop, without a timing-sensitive wall-clock sleep.
        let first_departure = Instant::now() - Duration::from_millis(1500);
        lease.renew_at = first_departure;
        let first = lease.due_at(first_departure).unwrap();
        let second = lease
            .due_at(first_departure + Duration::from_secs(1))
            .unwrap();
        assert_eq!(second, first + 1);

        lease.install(first, token(vm), 60).unwrap();
        assert_eq!(lease.installed_seq, first);
        assert!(lease.valid());
        assert!(lease.expires <= first_departure + Duration::from_secs(60));
        assert!(lease.pending.contains_key(&second));
        assert!(!lease.pending.contains_key(&first));
        assert!(lease.install(first, token(vm), 60).is_err());

        lease.install(second, token(vm), 60).unwrap();
        assert_eq!(lease.installed_seq, second);
        assert!(lease.pending.is_empty());
    }

    #[test]
    fn newer_reply_fences_older_outstanding_reply_and_expiry_refuses_revival() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        let departure = Instant::now() - Duration::from_millis(1500);
        lease.renew_at = departure;
        let first = lease.due_at(departure).unwrap();
        let second = lease.due_at(departure + Duration::from_secs(1)).unwrap();
        lease.install(second, token(vm), 60).unwrap();
        let installed_expiry = lease.expires;
        let installed_token = lease.token().to_owned();
        assert!(lease
            .install(first, token(uuid::Uuid::new_v4()), 60)
            .is_err());
        assert_eq!(lease.expires, installed_expiry);
        assert_eq!(lease.token(), installed_token);

        lease.renew_at = Instant::now();
        let third = lease.due().unwrap();
        lease.expires = Instant::now() - Duration::from_millis(1);
        assert!(lease.install(third, token(vm), 60).is_err());
        assert!(lease.due().is_none());
        assert_eq!(lease.installed_seq, second);
    }

    #[test]
    fn bounded_window_preserves_slow_requests_without_extending_lease() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        let now = Instant::now();
        let original_expiry = lease.expires;
        for expected in 1..=MAX_OUTSTANDING_RENEWALS {
            lease.renew_at = now;
            assert_eq!(lease.due_at(now), Some(expected as u64));
        }
        lease.renew_at = now;
        assert!(lease.due_at(now).is_none());
        assert_eq!(lease.pending.len(), MAX_OUTSTANDING_RENEWALS);
        assert_eq!(lease.expires, original_expiry);
        lease.install(1, token(vm), 60).unwrap();
        assert_eq!(lease.pending.len(), MAX_OUTSTANDING_RENEWALS - 1);
        lease.renew_at = Instant::now();
        assert_eq!(lease.due(), Some(MAX_OUTSTANDING_RENEWALS as u64 + 1));
    }

    #[test]
    fn peer_replacement_reopens_full_request_window_without_renewing_custody() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        let expiry = lease.expires;
        let current_token = lease.token().to_owned();
        for expected in 1..=MAX_OUTSTANDING_RENEWALS {
            let now = Instant::now();
            lease.renew_at = now;
            assert_eq!(lease.due_at(now), Some(expected as u64));
        }
        lease.renew_at = Instant::now();
        assert!(lease.due().is_none());

        lease.peer_replaced();
        assert!(lease.pending.is_empty());
        assert_eq!(lease.expires, expiry);
        assert_eq!(lease.token(), current_token);
        assert_eq!(lease.installed_seq, 0);
        assert_eq!(lease.seq, MAX_OUTSTANDING_RENEWALS as u64);
        let next = lease.due().expect("replacement peer can renew immediately");
        assert_eq!(next, MAX_OUTSTANDING_RENEWALS as u64 + 1);
        assert!(lease.install(1, token(vm), 60).is_err());
        assert!(lease
            .install(MAX_OUTSTANDING_RENEWALS as u64, token(vm), 60)
            .is_err());
        lease.install(next, token(vm), 60).unwrap();
        assert_eq!(lease.installed_seq, next);
    }

    #[test]
    fn peer_replacement_preserves_installed_sequence_and_cannot_revive_expiry() {
        let vm = uuid::Uuid::new_v4();
        let mut lease = Lease::new(token(vm), vm, 60).unwrap();
        lease.renew_at = Instant::now();
        let seq = lease.due().unwrap();
        lease.install(seq, token(vm), 60).unwrap();
        lease.expires = Instant::now() - Duration::from_millis(1);
        let expiry = lease.expires;
        let renewal = lease.renew_at;
        let current_token = lease.token().to_owned();

        lease.peer_replaced();
        assert_eq!(lease.expires, expiry);
        assert_eq!(lease.renew_at, renewal);
        assert_eq!(lease.token(), current_token);
        assert_eq!(lease.seq, seq);
        assert_eq!(lease.installed_seq, seq);
        assert!(lease.due().is_none());
        assert!(lease.install(seq, token(vm), 60).is_err());
    }
}
