//! Tests for `gateways::plist` (the XML property lists of `.tmPreferences`).

use interface_adapters::gateways::plist::parse;
use serde_json::json;

#[test]
fn a_tm_preferences_file_becomes_json() {
    let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <!-- a comment -->
    <key>scope</key>
    <string>source.python</string>
    <key>settings</key>
    <dict>
        <key>shellVariables</key>
        <array>
            <dict>
                <key>name</key><string>TM_COMMENT_START</string>
                <key>value</key><string># </string>
            </dict>
        </array>
        <key>showInSymbolList</key><integer>1</integer>
        <key>symbolTransformation</key><string><![CDATA[s/^\s*//;]]></string>
        <key>increaseIndentPattern</key><string>^\s*(if|for) .*:&lt;$</string>
        <key>enabled</key><true/>
        <key>ratio</key><real>0.5</real>
        <key>empty</key><string/>
    </dict>
</dict>
</plist>"#;
    assert_eq!(
        parse(source),
        Ok(json!({
            "scope": "source.python",
            "settings": {
                "shellVariables": [{ "name": "TM_COMMENT_START", "value": "# " }],
                "showInSymbolList": 1,
                "symbolTransformation": "s/^\\s*//;",
                "increaseIndentPattern": "^\\s*(if|for) .*:<$",
                "enabled": true,
                "ratio": 0.5,
                "empty": ""
            }
        }))
    );
}

#[test]
fn broken_files_are_errors() {
    assert!(parse("<plist><dict><key>a</key>").is_err());
    assert!(parse("<plist><dict><string>no key</string></dict></plist>").is_err());
}
