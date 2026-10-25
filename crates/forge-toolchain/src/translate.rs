use regex::Regex;
use std::sync::OnceLock;

struct Rule {
    re: Regex,
    to: &'static str,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |p: &str, to: &'static str| Rule {
            re: Regex::new(p).expect("gültiges Muster"),
            to,
        };
        vec![
            r(
                r"^expected ';' before '(.+)'( token)?$",
                "Semikolon erwartet vor „$1“",
            ),
            r(r"^expected ';' at end of (.+)$", "Semikolon fehlt am Ende"),
            r(
                r"^expected '([^']+)' before '(.+)'( token)?$",
                "„$1“ erwartet vor „$2“",
            ),
            r(
                r"^expected '([^']+)' at end of input$",
                "„$1“ fehlt am Ende der Datei",
            ),
            r(
                r"^expected primary-expression before '(.+)'( token)?$",
                "Ausdruck erwartet vor „$1“",
            ),
            r(
                r"^expected unqualified-id before '(.+)'( token)?$",
                "Name erwartet vor „$1“",
            ),
            r(
                r"^expected declaration before '(.+)'( token)?$",
                "Deklaration erwartet vor „$1“",
            ),
            r(
                r"^expected initializer before '(.+)'( token)?$",
                "Wert erwartet vor „$1“",
            ),
            r(
                r"^'(.+)' was not declared in this scope; did you mean '(.+)'\?$",
                "„$1“ ist nicht deklariert. Meintest du „$2“?",
            ),
            r(
                r"^'(.+)' was not declared in this scope$",
                "„$1“ ist nicht deklariert",
            ),
            r(r"^'(.+)' does not name a type$", "„$1“ ist kein Typ"),
            r(
                r"^unused variable '(.+)'$",
                "Variable „$1“ wird nicht verwendet",
            ),
            r(
                r"^variable '(.+)' set but not used$",
                "Variable „$1“ wird gesetzt, aber nie gelesen",
            ),
            r(
                r"^'(.+)' may be used uninitialized.*$",
                "„$1“ wird möglicherweise ohne Startwert gelesen",
            ),
            r(
                r"^no matching function for call to '(.+)'$",
                "Keine passende Funktion für den Aufruf „$1“",
            ),
            r(
                r"^invalid conversion from '(.+)' to '(.+)'.*$",
                "Ungültige Umwandlung von „$1“ in „$2“",
            ),
            r(
                r"^cannot convert '(.+)' to '(.+)'.*$",
                "„$1“ lässt sich nicht in „$2“ umwandeln",
            ),
            r(
                r"^too many arguments to function '(.+)'$",
                "Zu viele Argumente für „$1“",
            ),
            r(
                r"^too few arguments to function '(.+)'$",
                "Zu wenige Argumente für „$1“",
            ),
            r(r"^redefinition of '(.+)'$", "„$1“ ist doppelt definiert"),
            r(
                r"^conflicting declaration '(.+)'$",
                "Widersprüchliche Deklaration „$1“",
            ),
            r(
                r"^control reaches end of non-void function$",
                "Die Funktion endet ohne return",
            ),
            r(
                r"^no return statement in function returning non-void$",
                "Die Funktion hat einen Rückgabetyp, aber kein return",
            ),
            r(
                r"^comparison of integer expressions of different signedness.*$",
                "Vergleich von vorzeichenbehafteten und vorzeichenlosen Zahlen",
            ),
            r(
                r"^(.+): No such file or directory$",
                "Datei „$1“ nicht gefunden",
            ),
            r(r"^stray '(.+)' in program$", "Unerwartetes Zeichen „$1“"),
            r(
                r"^missing terminating (.) character$",
                "Zeichenkette ist nicht beendet",
            ),
            r(
                r"^'(.+)' is not a member of '(.+)'$",
                "„$1“ ist kein Mitglied von „$2“",
            ),
            r(
                r"^'(.+)' has no member named '(.+)'$",
                "„$1“ hat kein Mitglied „$2“",
            ),
            r(
                r"^lvalue required as left operand of assignment$",
                "Links vom = muss eine Variable stehen",
            ),
            r(
                r"^assignment of read-only variable '(.+)'$",
                "„$1“ ist schreibgeschützt",
            ),
            r(
                r"^invalid operands of types '(.+)' and '(.+)' to binary '(.+)'$",
                "Operator „$3“ passt nicht zu „$1“ und „$2“",
            ),
            r(
                r"^call of overloaded '(.+)' is ambiguous$",
                "Der Aufruf von „$1“ ist mehrdeutig",
            ),
            r(
                r"^too many initializers for '(.+)'$",
                "Zu viele Startwerte für „$1“",
            ),
            r(
                r"^deprecated conversion from string constant to '(.+)'.*$",
                "Zeichenketten sind konstant. Verwende const char*",
            ),
            r(
                r"^unused parameter '(.+)'$",
                "Parameter „$1“ wird nicht verwendet",
            ),
        ]
    })
}

/// Übersetzt häufige GCC-Meldungen. Unbekannte Meldungen bleiben im Original, damit nichts verloren geht.
pub fn translate(message: &str) -> String {
    let m = message.trim();
    for rule in rules() {
        if rule.re.is_match(m) {
            return rule.re.replace(m, rule.to).into_owned();
        }
    }
    m.to_string()
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn translates_common_messages() {
        assert_eq!(
            translate("expected ';' before 'digitalWrite'"),
            "Semikolon erwartet vor „digitalWrite“"
        );
        assert_eq!(
            translate("'foo' was not declared in this scope"),
            "„foo“ ist nicht deklariert"
        );
        assert_eq!(
            translate("'ledPin' was not declared in this scope; did you mean 'LED_PIN'?"),
            "„ledPin“ ist nicht deklariert. Meintest du „LED_PIN“?"
        );
        assert_eq!(
            translate("unused variable 'x'"),
            "Variable „x“ wird nicht verwendet"
        );
        assert_eq!(
            translate("Adafruit_NeoPixel.h: No such file or directory"),
            "Datei „Adafruit_NeoPixel.h“ nicht gefunden"
        );
        assert_eq!(
            translate("control reaches end of non-void function"),
            "Die Funktion endet ohne return"
        );
    }

    #[test]
    fn unknown_messages_stay_original() {
        assert_eq!(
            translate("something entirely different"),
            "something entirely different"
        );
    }
}
