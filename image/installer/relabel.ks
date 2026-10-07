# Included by the installer's interactive-defaults.ks and by the automated
# install test (tests/smoke/iso_install.py).
#
# Files Anaconda writes into the installed system (users and groups, hostname,
# locale, default target, ...) can end up with SELinux labels that the
# installed system's policy does not expect; with SELinux enforcing that broke
# rpm-ostreed.service's DynamicUser lookup (CI run #14: "Failed to update
# dynamic user credentials: Permission denied"). Relabel them with the
# installed system's own policy.
# Output goes to the console (the serial log in CI).
%post --log=/dev/console
set -u
relabel() {  # relabel POLICY_ROOT PATH_ROOT PATH...: label PATHs (seen as
            # paths below PATH_ROOT) as POLICY_ROOT's policy says
    fc="$1/etc/selinux/targeted/contexts/files/file_contexts"
    root=$2; shift 2
    [ -f "$fc" ] || { echo "mados-relabel: no $fc"; return 0; }
    setfiles -F -r "$root" "$fc" "$@" && echo "mados-relabel: relabelled $*"
}
if [ -d /ostree/deploy ]; then
    # Physical root: deployments live in /ostree/deploy/<os>/deploy/<id>,
    # their /var in /ostree/deploy/<os>/var.
    for dep in /ostree/deploy/*/deploy/*/; do
        dep=${dep%/}
        os=${dep%/deploy/*}
        [ -d "$dep/etc" ] && relabel "$dep" "$dep" "$dep/etc"
        [ -d "$os/var/home" ] && relabel "$dep" "$os" "$os/var/home"
    done
else
    # Running inside the installed deployment.
    restorecon -RF /etc && echo "mados-relabel: relabelled /etc"
    [ -d /var/home ] && restorecon -RF /var/home && echo "mados-relabel: relabelled /var/home"
fi
echo "mados-relabel: done"
%end
