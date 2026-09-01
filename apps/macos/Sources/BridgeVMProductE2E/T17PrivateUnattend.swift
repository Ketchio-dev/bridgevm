import Foundation
import Security

enum T17PrivateUnattend {
    static func write(to output: URL, nonce: String, fileManager: FileManager) throws {
        guard nonce.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil else {
            throw T17Blocker(code: "invalid-request", detail: "private E2E credential nonce is invalid")
        }
        var random = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else {
            throw T17Blocker(code: "internal-error", detail: "CSPRNG could not create private E2E credentials")
        }
        let password = Data(random).base64EncodedString()
        let username = "BVT17\(nonce.prefix(12))"
        let xml = """
        <?xml version="1.0" encoding="utf-8"?>
        <unattend xmlns="urn:schemas-microsoft-com:unattend" xmlns:wcm="http://schemas.microsoft.com/WMIConfig/2002/State">
          <settings pass="specialize"><component name="Microsoft-Windows-Shell-Setup" processorArchitecture="arm64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS"><ComputerName>BRIDGEVM</ComputerName><TimeZone>UTC</TimeZone></component></settings>
          <settings pass="oobeSystem">
            <component name="Microsoft-Windows-International-Core" processorArchitecture="arm64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS"><InputLocale>en-US</InputLocale><SystemLocale>en-US</SystemLocale><UILanguage>en-US</UILanguage><UserLocale>en-US</UserLocale></component>
            <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="arm64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
              <TimeZone>UTC</TimeZone><OOBE><HideEULAPage>true</HideEULAPage><ProtectYourPC>3</ProtectYourPC></OOBE>
              <UserAccounts><LocalAccounts><LocalAccount wcm:action="add"><Name>\(username)</Name><Group>Administrators</Group><Password><Value>\(password)</Value><PlainText>true</PlainText></Password></LocalAccount></LocalAccounts></UserAccounts>
              <AutoLogon><Enabled>true</Enabled><LogonCount>4</LogonCount><Username>\(username)</Username><Password><Value>\(password)</Value><PlainText>true</PlainText></Password></AutoLogon>
              <FirstLogonCommands><SynchronousCommand wcm:action="add"><Order>1</Order><CommandLine>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVM\\provisioning\\agent\\bvagent-firstboot.ps1</CommandLine><Description>Verify and start the sealed BridgeVM guest agent for the isolated T17 run</Description></SynchronousCommand></FirstLogonCommands>
            </component>
          </settings>
        </unattend>
        """.replacingOccurrences(of: "\n", with: "\r\n")
        try Data(xml.utf8).write(to: output, options: [.withoutOverwriting])
        try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: output.path)
    }
}
