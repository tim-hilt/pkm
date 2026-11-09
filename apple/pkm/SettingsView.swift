import SwiftUI
import UniformTypeIdentifiers

/// The Settings: Appearance and Cloud Backing. A window on the Mac (⌘,), a
/// sheet from the Gallery's gear on iPhone and iPad.
struct SettingsView: View {
    var body: some View {
        Form {
            Section { AppearancePicker() }
            CloudBackingSection()
        }
        .formStyle(.grouped)
        #if os(macOS)
            .frame(width: 460, height: 460)
        #endif
    }
}

/// Cloud Backing: pick a Cloud Provider, enter the WebDAV server's URL,
/// username and password, or grant iCloud Drive, and Connect; once Connected,
/// see where the Drawings go and Disconnect.
struct CloudBackingSection: View {
    @Environment(CloudBackingModel.self) private var cloud

    @State private var provider = CloudProvider.webDAV
    @State private var url = ""
    @State private var username = ""
    @State private var password = ""
    @State private var error: String?
    @State private var isConnecting = false
    @State private var isPickingFolder = false

    var body: some View {
        Section("Cloud Backing") {
            Picker("Cloud Provider", selection: pickedProvider) {
                ForEach(CloudProvider.allCases, id: \.self) { provider in
                    Label {
                        Text(provider.name)
                    } icon: {
                        provider.icon
                    }
                    .tag(provider)
                }
            }
            .accessibilityIdentifier("cloud-provider")
            if let connection = cloud.connection {
                connected(to: connection)
            } else if provider == .iCloudDrive {
                iCloudDriveForm
            } else if provider == .googleDrive {
                googleDriveForm
            } else if provider == .oneDrive {
                oneDriveForm
            } else {
                webDAVForm
            }
        }
        .fileImporter(isPresented: $isPickingFolder, allowedContentTypes: [.folder]) { result in
            granted(result)
        }
        .confirmationDialog(
            "Disconnect \(cloud.connection?.provider.name ?? "")?", isPresented: isSwitching,
            titleVisibility: .visible
        ) {
            Button("Disconnect", role: .destructive) {
                if let picked = cloud.confirmSwitch() { provider = picked }
            }
            .accessibilityIdentifier("cloud-switch-confirm")
            Button("Cancel", role: .cancel) { cloud.cancelSwitch() }
        } message: {
            Text("Your Drawings stay on this Device. Connect \(cloud.pendingSwitch?.name ?? "") to back them up there.")
        }
    }

    /// The Connected Cloud Provider; picking another asks to Disconnect first.
    private var pickedProvider: Binding<CloudProvider> {
        Binding(
            get: { cloud.connection?.provider ?? provider },
            set: { picked in
                if cloud.connection == nil { provider = picked } else { cloud.requestSwitch(to: picked) }
            })
    }

    private var isSwitching: Binding<Bool> {
        Binding(get: { cloud.pendingSwitch != nil }, set: { if !$0 { cloud.cancelSwitch() } })
    }

    @ViewBuilder private func connected(to connection: CloudConnection) -> some View {
        LabeledContent {
            Text(connection.detail).textSelection(.enabled)
        } label: {
            Label {
                Text(connection.provider.detailLabel)
            } icon: {
                connection.provider.icon
            }
        }
        .accessibilityIdentifier("cloud-connected")
        LabeledContent("Status") {
            HStack {
                if cloud.hasProblem { Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange) }
                Text(cloud.statusText)
            }
        }
        .accessibilityIdentifier("cloud-status")
        if let lastSync = cloud.lastSync {
            LabeledContent("Last Sync") { Text(lastSync, format: .dateTime.hour().minute().second()) }
                .accessibilityIdentifier("cloud-last-sync")
        }
        if cloud.status == .accessLost, connection.provider == .iCloudDrive {
            if let error {
                Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
            }
            Button("Reconnect", action: pickFolder)
                .disabled(isConnecting)
                .accessibilityIdentifier("cloud-reconnect")
        } else if cloud.status == .accessLost, connection.provider == .googleDrive || connection.provider == .oneDrive {
            if let error {
                Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
            }
            Button("Reconnect") { signIn(to: connection.provider) }
                .disabled(isConnecting)
                .accessibilityIdentifier("cloud-reconnect")
        } else if cloud.status == .accessLost {
            SecureField("Password", text: $password)
                .accessibilityIdentifier("cloud-password")
            if let error {
                Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
            }
            Button("Reconnect", action: reconnect)
                .disabled(isConnecting || password.isEmpty)
                .accessibilityIdentifier("cloud-reconnect")
        }
        Button("Disconnect", role: .destructive) { cloud.disconnect() }
            .accessibilityIdentifier("cloud-disconnect")
    }

    @ViewBuilder private var webDAVForm: some View {
        TextField("Server URL", text: $url, prompt: Text(verbatim: "https://example.com/dav"))
            .autocorrectionDisabled()
            .accessibilityIdentifier("cloud-url")
            #if os(iOS)
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
            #endif
        TextField("Username", text: $username)
            .autocorrectionDisabled()
            .accessibilityIdentifier("cloud-username")
            #if os(iOS)
                .textInputAutocapitalization(.never)
            #endif
        SecureField("Password", text: $password)
            .accessibilityIdentifier("cloud-password")
        if let error {
            Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
        }
        Button("Connect", action: connect)
            .disabled(isConnecting || url.isEmpty || username.isEmpty || password.isEmpty)
            .accessibilityIdentifier("cloud-connect")
    }

    @ViewBuilder private var iCloudDriveForm: some View {
        Text("You are asked once to grant iCloud Drive. Your Drawings are kept in its pkm folder.")
            .font(.footnote).foregroundStyle(.secondary)
        if let error {
            Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
        }
        Button("Connect", action: pickFolder)
            .disabled(isConnecting)
            .accessibilityIdentifier("cloud-connect")
    }

    @ViewBuilder private var googleDriveForm: some View {
        Text(
            "You sign in on Google's page. Your Drawings are kept in the pkm folder of My Drive; the app sees only files it created."
        )
        .font(.footnote).foregroundStyle(.secondary)
        if let error {
            Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
        }
        Button("Connect") { signIn(to: .googleDrive) }
            .disabled(isConnecting)
            .accessibilityIdentifier("cloud-connect")
    }

    @ViewBuilder private var oneDriveForm: some View {
        Text(
            "You sign in on Microsoft's page. Your Drawings are kept in Apps/pkm of your OneDrive; the app sees nothing else. Personal Microsoft accounts only."
        )
        .font(.footnote).foregroundStyle(.secondary)
        if let error {
            Text(error).foregroundStyle(.red).accessibilityIdentifier("cloud-error")
        }
        Button("Connect") { signIn(to: .oneDrive) }
            .disabled(isConnecting)
            .accessibilityIdentifier("cloud-connect")
    }

    /// Signs in to Google Drive or OneDrive, to Connect or Reconnect. Closing
    /// the sign-in page does nothing.
    private func signIn(to provider: CloudProvider) {
        Task {
            isConnecting = true
            defer { isConnecting = false }
            do {
                if provider == .oneDrive {
                    try await cloud.connectOneDrive()
                } else {
                    try await cloud.connectGoogleDrive()
                }
                error = nil
            } catch let failure as ConnectError {
                error = failure.message
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func pickFolder() {
        error = nil
        isPickingFolder = true
    }

    /// The user picked a folder, or backed out of the picker, which does nothing.
    private func granted(_ result: Result<URL, Error>) {
        guard case .success(let folder) = result else { return }
        Task {
            isConnecting = true
            defer { isConnecting = false }
            do {
                try await cloud.connectICloudDrive(granted: folder)
                error = nil
            } catch let failure as ConnectError {
                error = failure.message
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func reconnect() {
        Task {
            isConnecting = true
            defer { isConnecting = false }
            do {
                try await cloud.reconnect(password: password)
                (error, password) = (nil, "")
            } catch let failure as ConnectError {
                error = failure.message
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func connect() {
        Task {
            isConnecting = true
            defer { isConnecting = false }
            do {
                try await cloud.connect(url: url, username: username, password: password)
                (error, password) = (nil, "")
            } catch let failure as ConnectError {
                error = failure.message
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

#if os(iOS)
    /// `SettingsView` in a sheet with a Done button.
    struct SettingsSheet: View {
        @Environment(\.dismiss) private var dismiss

        var body: some View {
            NavigationStack {
                SettingsView()
                    .navigationTitle("Settings")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar { Button("Done") { dismiss() } }
            }
        }
    }
#endif
