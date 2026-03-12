FROM jenkins/jenkins:lts-jdk21

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    jq \
    && rm -rf /var/lib/apt/lists/*

COPY controller/plugins.txt /usr/share/jenkins/ref/plugins.txt
RUN jenkins-plugin-cli --plugin-file /usr/share/jenkins/ref/plugins.txt

COPY controller/casc/ /opt/jenkins/casc/
COPY jobs/ /opt/jenkins/jobs/
COPY controller/support/ /opt/jenkins/support/
RUN chmod +x /opt/jenkins/support/*.sh \
    && chown -R jenkins:jenkins /opt/jenkins/casc /opt/jenkins/jobs /opt/jenkins/support

ENV CASC_JENKINS_CONFIG="/opt/jenkins/casc/jenkins.yaml"

USER jenkins
