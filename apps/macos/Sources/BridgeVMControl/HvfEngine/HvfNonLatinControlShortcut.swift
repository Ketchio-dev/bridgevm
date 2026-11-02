import SwiftUI

extension HvfHostKeyCommand {
    /// Physical Ctrl+Return/Tab first, then logical commands; non-Latin Ctrl letters may instead identify an ANSI letter position.
    static func resolve(characters: String, modifiers: EventModifiers, keyCode: UInt16) -> Self {
        let flags = modifiers.intersection([.control, .option, .shift, .command])
        if flags == .control || flags == [.control, .shift],
           let key = HvfControlChordKeys.physical(shift: flags.contains(.shift), keyCode: keyCode) { return .key(key) }
        let logical = resolve(characters: characters, modifiers: modifiers)
        guard logical == .ignored, modifiers == .control, characters.unicodeScalars.count == 1,
              let scalar = characters.unicodeScalars.first, !scalar.isASCII,
              nonLatinControlLetters.contains(scalar.properties.generalCategory),
              let key = HvfControlChordKeys.letterPositions[keyCode] else { return logical }
        return .key("ctrl+" + key)
    }

    private static let nonLatinControlLetters: [Unicode.GeneralCategory] = [.uppercaseLetter, .lowercaseLetter, .titlecaseLetter, .modifierLetter, .otherLetter]
}
