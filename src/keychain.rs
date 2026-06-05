use core_foundation::base::{TCFType, ToVoid};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFMutableDictionary;
use core_foundation::string::CFString;
use security_framework::item::{
    ItemClass, ItemSearchOptions, ItemUpdateOptions, ItemUpdateValue, Limit, SearchResult,
};
use security_framework_sys::base::errSecItemNotFound;
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrDescription, kSecAttrLabel, kSecAttrService,
    kSecAttrSynchronizable, kSecClass, kSecClassGenericPassword, kSecValueData,
};
use security_framework_sys::keychain_item::SecItemAdd;
use std::collections::BTreeSet;
use std::fmt;

const SERVICE_PREFIX: &str = "keyser-";
const ITEM_DESCRIPTION: &str = "keyser";

#[derive(Debug)]
pub enum KeyserError {
    Keychain(security_framework::base::Error),
    ItemNotFound,
}

impl fmt::Display for KeyserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyserError::Keychain(e) => write!(f, "Keychain error: {e}"),
            KeyserError::ItemNotFound => write!(f, "Item not found"),
        }
    }
}

impl std::error::Error for KeyserError {}

impl From<security_framework::base::Error> for KeyserError {
    fn from(e: security_framework::base::Error) -> Self {
        if e.code() == errSecItemNotFound {
            KeyserError::ItemNotFound
        } else {
            KeyserError::Keychain(e)
        }
    }
}

pub type Result<T> = std::result::Result<T, KeyserError>;

fn service_name(namespace: &str) -> String {
    format!("{SERVICE_PREFIX}{namespace}")
}

fn label_name(namespace: &str, key: &str) -> String {
    format!("{SERVICE_PREFIX}{namespace}-{key}")
}

/// Save (create or update) a generic password in the keychain.
pub fn save_value(
    namespace: &str,
    key: &str,
    value: &str,
    _require_passphrase: Option<bool>,
) -> Result<()> {
    let svc = service_name(namespace);
    let lbl = label_name(namespace, key);
    let data = CFData::from_buffer(value.as_bytes());

    // Try to update an existing item first.
    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(&svc)
        .account(key);

    let mut update = ItemUpdateOptions::new();
    update.set_label(&lbl);
    update.set_description(ITEM_DESCRIPTION);
    update.set_value(ItemUpdateValue::Data(data.clone()));

    match security_framework::item::update_item(&search, &update) {
        Ok(()) => return Ok(()),
        Err(e) if e.code() == errSecItemNotFound => {
            // Item doesn't exist yet — fall through to create.
        }
        Err(e) => return Err(e.into()),
    }

    // Create a new item using raw SecItemAdd so we can set kSecAttrSynchronizable = false.
    unsafe {
        let mut dict = CFMutableDictionary::from_CFType_pairs(&[]);
        dict.add(&kSecClass.to_void(), &kSecClassGenericPassword.to_void());
        dict.add(
            &kSecAttrService.to_void(),
            &CFString::new(&svc).to_void(),
        );
        dict.add(
            &kSecAttrAccount.to_void(),
            &CFString::new(key).to_void(),
        );
        dict.add(
            &kSecAttrLabel.to_void(),
            &CFString::new(&lbl).to_void(),
        );
        dict.add(
            &kSecAttrDescription.to_void(),
            &CFString::new(ITEM_DESCRIPTION).to_void(),
        );
        dict.add(&kSecValueData.to_void(), &data.to_void());
        dict.add(
            &kSecAttrSynchronizable.to_void(),
            &CFBoolean::false_value().to_void(),
        );

        let status = SecItemAdd(dict.to_immutable().as_concrete_TypeRef(), std::ptr::null_mut());
        if status != 0 {
            return Err(security_framework::base::Error::from_code(status).into());
        }
    }

    Ok(())
}

/// Search for all namespaces that have keyser items.
pub fn search_namespaces() -> Result<Vec<String>> {
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .limit(Limit::All)
        .load_attributes(true)
        .search();

    let items = match results {
        Ok(items) => items,
        Err(e) if e.code() == errSecItemNotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let mut namespaces = BTreeSet::new();

    for item in &items {
        if let SearchResult::Dict(_) = item {
            let map = match item.simplify_dict() {
                Some(m) => m,
                None => continue,
            };

            // Filter: only items with description == "keyser"
            let desc = match map.get("desc") {
                Some(d) => d.as_str(),
                None => continue,
            };
            if desc != ITEM_DESCRIPTION {
                continue;
            }

            // Extract namespace from service name
            let svc = match map.get("svce") {
                Some(s) => s.as_str(),
                None => continue,
            };
            if let Some(ns) = svc.strip_prefix(SERVICE_PREFIX) {
                namespaces.insert(ns.to_string());
            }
        }
    }

    Ok(namespaces.into_iter().collect())
}

/// Search for all key-value pairs in a namespace.
///
/// macOS Keychain does not allow kSecReturnAttributes + kSecReturnData +
/// kSecMatchLimitAll in a single query, so we first list accounts (attributes
/// only) and then fetch each item's data individually.
pub fn search_values(namespace: &str) -> Result<Vec<(String, String)>> {
    let svc = service_name(namespace);

    // Step 1: get all accounts in this service (attributes only).
    let results = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(&svc)
        .limit(Limit::All)
        .load_attributes(true)
        .search();

    let items = match results {
        Ok(items) => items,
        Err(e) if e.code() == errSecItemNotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    let mut accounts = BTreeSet::new();
    for item in &items {
        if let Some(map) = item.simplify_dict() {
            if let Some(acct) = map.get("acct") {
                accounts.insert(acct.clone());
            }
        }
    }

    // Step 2: for each account, fetch the data.
    let mut pairs = Vec::new();
    for account in &accounts {
        let data_results = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(&svc)
            .account(account)
            .load_data(true)
            .search();

        match data_results {
            Ok(data_items) => {
                for data_item in &data_items {
                    if let SearchResult::Data(bytes) = data_item {
                        let value = String::from_utf8_lossy(bytes).to_string();
                        pairs.push((account.clone(), value));
                        break;
                    }
                }
            }
            Err(e) if e.code() == errSecItemNotFound => continue,
            Err(e) => return Err(e.into()),
        }
    }

    // Already sorted by BTreeSet, but be explicit
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

/// Delete a key from a namespace. Treats "not found" as success (idempotent).
pub fn delete_value(namespace: &str, key: &str) -> Result<()> {
    let svc = service_name(namespace);

    let mut search = ItemSearchOptions::new();
    search
        .class(ItemClass::generic_password())
        .service(&svc)
        .account(key);

    match search.delete() {
        Ok(()) => Ok(()),
        Err(e) if e.code() == errSecItemNotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
