use crate::{Contact, OckamError};
use hashbrown::HashMap;
use ockam_vault_core::{Hasher, KeyIdVault, PublicKey, Secret, SecretVault, Signer, Verifier};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

mod identifiers;
pub use identifiers::*;
mod key_attributes;
pub use key_attributes::*;
mod change;
use crate::history::ProfileChangeHistory;
pub use change::*;

pub trait ProfileVault: SecretVault + KeyIdVault + Hasher + Signer + Verifier {}

impl<D> ProfileVault for D where D: SecretVault + KeyIdVault + Hasher + Signer + Verifier {}

pub type ProfileEventAttributes = HashMap<String, String>;

/// Profile is an abstraction responsible for keeping, verifying and modifying
/// user's data (mainly - public keys). It is used to create new keys, rotate and revoke them.
/// Public keys together with metadata will be organised into events chain, corresponding
/// secret keys will be saved into the given Vault implementation. Events chain and corresponding
/// secret keys are what fully determines Profile.
///
///
/// # Examples
/// ```
/// use ockam_vault::SoftwareVault;
/// use std::sync::{Mutex, Arc};
/// use ockam::{Profile, KeyAttributes, ProfileKeyType, ProfileKeyPurpose};
///
/// fn example() {
///     let vault = SoftwareVault::default();
///     let vault = Arc::new(Mutex::new(vault));
///     let mut profile = Profile::create(None, vault).unwrap();
///
///     let root_key_attributes = KeyAttributes::new(
///         Profile::ROOT_KEY_LABEL.to_string(),
///         ProfileKeyType::Root,
///         ProfileKeyPurpose::ProfileUpdate,
///     );
///
///     let _alice_root_secret = profile.get_secret_key(&root_key_attributes).unwrap();
///
///     let truck_key_attributes = KeyAttributes::new(
///         "Truck management".to_string(),
///         ProfileKeyType::Issuing,
///         ProfileKeyPurpose::IssueCredentials,
///     );
///
///     profile
///         .create_key(truck_key_attributes.clone(), None)
///         .unwrap();
///
///     let _alice_truck_secret = profile.get_secret_key(&truck_key_attributes).unwrap();
///
///     profile.rotate_key(truck_key_attributes.clone(), None).unwrap();
///
///     let _alice_truck_secret = profile.get_secret_key(&truck_key_attributes).unwrap();
///
///     profile.verify().unwrap();
/// }
/// ```
#[derive(Clone)]
pub struct Profile {
    identifier: ProfileIdentifier,
    change_history: ProfileChangeHistory,
    vault: Arc<Mutex<dyn ProfileVault>>,
}

impl Profile {
    pub const NO_EVENT: &'static [u8] = "OCKAM_NO_EVENT".as_bytes();
    pub const ROOT_KEY_LABEL: &'static str = "OCKAM_PRK";
    pub const CURRENT_VERSION: u8 = 1;
    pub const CHANGE_CURRENT_VERSION: u8 = 1;
}

impl Profile {
    /// Return unique identifier, which equals to sha256 of the root public key
    pub fn identifier(&self) -> &ProfileIdentifier {
        &self.identifier
    }
    /// Return change history chain
    pub fn change_events(&self) -> &[ProfileChangeEvent] {
        self.change_history.as_ref()
    }
}

impl Profile {
    pub fn new(
        identifier: ProfileIdentifier,
        change_events: Vec<ProfileChangeEvent>,
        vault: Arc<Mutex<dyn ProfileVault>>,
    ) -> Self {
        let profile = Self {
            identifier,
            change_history: ProfileChangeHistory::new(change_events),
            vault,
        };

        profile
    }
}

impl Profile {
    /// Generate fresh root key and create new [`Profile`]
    pub fn create(
        attributes: Option<ProfileEventAttributes>,
        vault: Arc<Mutex<dyn ProfileVault>>,
    ) -> ockam_core::Result<Self> {
        let mut v = vault.lock().unwrap();
        let prev_id = v.sha256(Profile::NO_EVENT)?;
        let prev_id = EventIdentifier::from_hash(prev_id);

        let key_attributes = KeyAttributes::new(
            Profile::ROOT_KEY_LABEL.to_string(),
            ProfileKeyType::Root,
            ProfileKeyPurpose::ProfileUpdate,
        );
        let change_event = Self::create_key_event_static(
            prev_id,
            key_attributes.clone(),
            attributes,
            None,
            v.deref_mut(),
        )?;

        let change = ProfileChangeHistory::find_key_change_in_event(&change_event, &key_attributes)
            .ok_or(OckamError::InvalidInternalState)?;
        let public_key = ProfileChangeHistory::get_change_public_key(&change)?;

        let public_kid = v.compute_key_id_for_public_key(&public_key)?;
        let public_kid = ProfileIdentifier::from_key_id(public_kid);

        let profile = Profile::new(public_kid, vec![change_event], vault.clone());

        Ok(profile)
    }

    /// Create new key
    /// Key is uniquely identified by (label, key_type, key_purpose) triplet in [`KeyAttributes`]
    pub fn create_key(
        &mut self,
        key_attributes: KeyAttributes,
        attributes: Option<ProfileEventAttributes>,
    ) -> ockam_core::Result<()> {
        let root_secret = self.get_root_secret()?;
        let event = self.create_key_event(key_attributes, attributes, Some(&root_secret))?;
        self.apply_no_verification(event)
    }

    /// Rotate existing key
    /// Key is uniquely identified by (label, key_type, key_purpose) triplet in [`KeyAttributes`]
    pub fn rotate_key(
        &mut self,
        key_attributes: KeyAttributes,
        attributes: Option<ProfileEventAttributes>,
    ) -> ockam_core::Result<()> {
        let root_secret = self.get_root_secret()?;
        let event = self.rotate_key_event(key_attributes, attributes, &root_secret)?;
        self.apply_no_verification(event)
    }

    /// Get [`Secret`] key. Key is uniquely identified by (label, key_type, key_purpose) triplet in [`KeyAttributes`]
    pub fn get_secret_key(&self, key_attributes: &KeyAttributes) -> ockam_core::Result<Secret> {
        let event = self.change_history.find_last_key_event(key_attributes)?;
        Self::get_secret_key_from_event(key_attributes, event, self.vault.lock().unwrap().deref())
    }

    /// Get [`PublicKey`]. Key is uniquely identified by (label, key_type, key_purpose) triplet in [`KeyAttributes`]
    pub fn get_public_key(&self, key_attributes: &KeyAttributes) -> ockam_core::Result<PublicKey> {
        self.change_history.get_public_key(key_attributes)
    }

    /// Return change history chain after event with given [`EventIdentifier`]
    pub fn get_events_after(
        &self,
        id: &EventIdentifier,
    ) -> ockam_core::Result<&[ProfileChangeEvent]> {
        let pos = self
            .change_events()
            .iter()
            .rev()
            .position(|e| e.identifier() == id)
            .ok_or(OckamError::EventNotFound)?;
        Ok(&self.change_events()[pos + 1..])
    }
}

impl Profile {
    fn apply_no_verification(
        &mut self,
        change_event: ProfileChangeEvent,
    ) -> ockam_core::Result<()> {
        let slice = std::slice::from_ref(&change_event);
        ProfileChangeHistory::check_consistency(self.change_events(), &slice)?;
        self.change_history.push_event(change_event);

        Ok(())
    }

    /// Apply new change to the [`Profile`]. Change will be cryptographically verified
    pub fn apply(&mut self, change_event: ProfileChangeEvent) -> ockam_core::Result<()> {
        self.verify_event(&change_event)?;

        self.apply_no_verification(change_event)
    }

    /// Verify cryptographically event relative to current [`Profile`]'s event chain.
    /// WARNING: This function assumes all existing events in chain are verified.
    fn verify_event(&self, change_event: &ProfileChangeEvent) -> ockam_core::Result<()> {
        let change_events = std::slice::from_ref(change_event);
        ProfileChangeHistory::check_consistency(self.change_events(), change_events)?;

        let mut vault = self.vault.lock().unwrap();

        self.change_history
            .verify_event(change_event, vault.deref_mut())
    }

    /// Verify whole event chain
    pub fn verify(&self) -> ockam_core::Result<()> {
        ProfileChangeHistory::check_consistency(&[], self.change_events())?;

        let mut vault = self.vault.lock().unwrap();

        for change_event in self.change_events().as_ref() {
            self.change_history
                .verify_event(change_event, vault.deref_mut())?;
        }

        Ok(())
    }
}

impl Profile {
    pub(crate) fn get_root_secret(&self) -> ockam_core::Result<Secret> {
        let public_key = self.change_history.get_root_public_key()?;

        let vault = self.vault.lock().unwrap();
        let key_id = vault.compute_key_id_for_public_key(&public_key)?;
        vault.get_secret_by_key_id(&key_id)
    }

    pub(crate) fn get_secret_key_from_event(
        key_attributes: &KeyAttributes,
        event: &ProfileChangeEvent,
        vault: &dyn ProfileVault,
    ) -> ockam_core::Result<Secret> {
        let public_key = ProfileChangeHistory::get_public_key_from_event(key_attributes, event)?;

        let public_kid = vault.compute_key_id_for_public_key(&public_key)?;

        vault.get_secret_by_key_id(&public_kid)
    }
}

impl Profile {
    pub fn to_contact(&self) -> Contact {
        Contact::new(
            self.identifier.clone(),
            self.change_history.as_ref().to_vec(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use ockam_vault::SoftwareVault;

    #[test]
    fn test_new() {
        let vault = SoftwareVault::default();
        let vault = Arc::new(Mutex::new(vault));
        let mut profile = Profile::create(None, vault).unwrap();

        let root_key_attributes = KeyAttributes::new(
            Profile::ROOT_KEY_LABEL.to_string(),
            ProfileKeyType::Root,
            ProfileKeyPurpose::ProfileUpdate,
        );

        let _alice_root_secret = profile.get_secret_key(&root_key_attributes).unwrap();
        let _alice_root_public_key = profile.get_public_key(&root_key_attributes).unwrap();

        let truck_key_attributes = KeyAttributes::new(
            "Truck management".to_string(),
            ProfileKeyType::Issuing,
            ProfileKeyPurpose::IssueCredentials,
        );

        profile
            .create_key(truck_key_attributes.clone(), None)
            .unwrap();

        let _alice_truck_secret = profile.get_secret_key(&truck_key_attributes).unwrap();
        let _alice_truck_public_key = profile.get_public_key(&truck_key_attributes).unwrap();

        profile
            .rotate_key(truck_key_attributes.clone(), None)
            .unwrap();

        let _alice_truck_secret = profile.get_secret_key(&truck_key_attributes).unwrap();
        let _alice_truck_public_key = profile.get_public_key(&truck_key_attributes).unwrap();

        profile.verify().unwrap();
    }
}
