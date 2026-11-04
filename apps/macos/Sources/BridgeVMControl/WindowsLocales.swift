/// Windows locale names for the answer file's UserLocale and SystemLocale. Every
/// name has an ANSI code page, so it is valid as a SystemLocale: a name Windows
/// does not know would stop Setup, so regions outside the table stay en-US.
enum WindowsLocales {
    /// The main locale of a region (ISO 3166-1 code).
    static let byRegion: [String: String] = [
        "AE": "ar-AE", "AR": "es-AR", "AT": "de-AT", "AU": "en-AU", "BE": "nl-BE", "BG": "bg-BG",
        "BR": "pt-BR", "CA": "en-CA", "CH": "de-CH", "CL": "es-CL", "CN": "zh-CN", "CO": "es-CO",
        "CZ": "cs-CZ", "DE": "de-DE", "DK": "da-DK", "EE": "et-EE", "EG": "ar-EG", "ES": "es-ES",
        "FI": "fi-FI", "FR": "fr-FR", "GB": "en-GB", "GR": "el-GR", "HK": "zh-HK", "HR": "hr-HR",
        "HU": "hu-HU", "ID": "id-ID", "IE": "en-IE", "IL": "he-IL", "IN": "en-IN", "IS": "is-IS",
        "IT": "it-IT", "JP": "ja-JP", "KR": "ko-KR", "LT": "lt-LT", "LV": "lv-LV", "MO": "zh-MO",
        "MX": "es-MX", "MY": "ms-MY", "NL": "nl-NL", "NO": "nb-NO", "NZ": "en-NZ", "PE": "es-PE",
        "PH": "en-PH", "PL": "pl-PL", "PT": "pt-PT", "RO": "ro-RO", "RS": "sr-Latn-RS", "RU": "ru-RU",
        "SA": "ar-SA", "SE": "sv-SE", "SG": "en-SG", "SI": "sl-SI", "SK": "sk-SK", "TH": "th-TH",
        "TR": "tr-TR", "TW": "zh-TW", "UA": "uk-UA", "US": "en-US", "VN": "vi-VN", "ZA": "en-ZA"
    ]

    /// Other languages Windows has in one of those regions.
    static let otherLanguages: Set<String> = [
        "ca-ES", "es-US", "eu-ES", "fr-BE", "fr-CA", "fr-CH", "gl-ES", "it-CH", "zh-SG"
    ]
}
