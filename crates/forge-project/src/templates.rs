use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Blank,
    Blink,
    Wifi,
    Webserver,
}

pub struct Rendered {
    pub main: String,
    pub secrets: &'static [&'static str],
}

impl Template {
    pub fn render(self, name: &str) -> Rendered {
        match self {
            Template::Blank => Rendered {
                main: format!("// {name}\n\nvoid setup() {{\n}}\n\nvoid loop() {{\n}}\n"),
                secrets: &[],
            },
            Template::Blink => Rendered {
                main: format!(
                    "// {name}\n// Lässt die eingebaute LED im Sekundentakt blinken.\n\n\
#ifndef LED_BUILTIN\n#define LED_BUILTIN 2\n#endif\n\n\
void setup() {{\n  pinMode(LED_BUILTIN, OUTPUT);\n}}\n\n\
void loop() {{\n  digitalWrite(LED_BUILTIN, HIGH);\n  delay(500);\n  digitalWrite(LED_BUILTIN, LOW);\n  delay(500);\n}}\n"
                ),
                secrets: &[],
            },
            Template::Wifi => Rendered {
                main: format!(
                    "// {name}\n// Verbindet sich mit dem WLAN. Zugangsdaten liegen im Schlüsselbund, nicht im Code.\n\n\
#include <WiFi.h>\n#include <ForgeSecrets.h>\n\n\
void setup() {{\n  Serial.begin(115200);\n  WiFi.begin(Secret(\"WIFI_SSID\").c_str(), Secret(\"WIFI_PASS\").c_str());\n\n  Serial.print(\"Verbinde mit WLAN\");\n  while (WiFi.status() != WL_CONNECTED) {{\n    delay(250);\n    Serial.print('.');\n  }}\n  Serial.println();\n  Serial.print(\"IP-Adresse: \");\n  Serial.println(WiFi.localIP());\n}}\n\n\
void loop() {{\n}}\n"
                ),
                secrets: &["WIFI_SSID", "WIFI_PASS"],
            },
            Template::Webserver => Rendered {
                main: format!(
                    "// {name}\n// Kleiner Webserver. Erreichbar unter der IP-Adresse aus dem Monitor.\n\n\
#include <WiFi.h>\n#include <WebServer.h>\n#include <ForgeSecrets.h>\n\n\
WebServer server(80);\n\n\
void handleRoot() {{\n  server.send(200, \"text/plain\", \"Hallo von Forge\");\n}}\n\n\
void setup() {{\n  Serial.begin(115200);\n  WiFi.begin(Secret(\"WIFI_SSID\").c_str(), Secret(\"WIFI_PASS\").c_str());\n\n  while (WiFi.status() != WL_CONNECTED) {{\n    delay(250);\n  }}\n  Serial.print(\"Erreichbar unter http://\");\n  Serial.println(WiFi.localIP());\n\n  server.on(\"/\", handleRoot);\n  server.begin();\n}}\n\n\
void loop() {{\n  server.handleClient();\n}}\n"
                ),
                secrets: &["WIFI_SSID", "WIFI_PASS"],
            },
        }
    }
}
