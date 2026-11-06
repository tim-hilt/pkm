import SwiftUI

/// The Settings: only Cloud Backing so far. A window on the Mac (⌘,), a sheet
/// from the Gallery's gear on iPhone and iPad.
struct SettingsView: View {
    var body: some View {
        Form { CloudBackingSection() }
            .formStyle(.grouped)
            #if os(macOS)
                .frame(width: 460, height: 460)
            #endif
    }
}

/// Cloud Backing: pick a Cloud Provider, enter the WebDAV server's URL,
/// username and password and Connect; once Connected, see where the Drawings
/// go and Disconnect.
struct CloudBackingSection: View {
    @Environment(CloudBackingModel.self) private var cloud

    @State private var provider = CloudProvider.webDAV
    @State private var url = ""
    @State private var username = ""
    @State private var password = ""
    @State private var error: String?
    @State private var isConnecting = false

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
                    .disabled(!provider.isAvailable)
                }
            }
            .accessibilityIdentifier("cloud-provider")
            if let connection = cloud.connection {
                connected(to: connection)
            } else {
                webDAVForm
            }
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
        if cloud.status == .accessLost {
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
